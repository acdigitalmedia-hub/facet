# Gives unweighted vertices full weight on the first joint and renormalizes the rest.
import json, struct, sys, os
CT = {5126: ('f', 4), 5121: ('B', 1), 5123: ('H', 2), 5125: ('I', 4)}
NC = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4}
for path in sys.argv[1:]:
    d = json.load(open(path))
    binpath = os.path.join(os.path.dirname(path), d['buffers'][0]['uri'])
    buf = bytearray(open(binpath, 'rb').read())
    fixed = 0
    for mesh in d['meshes']:
        for prim in mesh['primitives']:
            a = prim['attributes']
            if 'WEIGHTS_0' not in a:
                continue
            acc = d['accessors'][a['WEIGHTS_0']]
            bv = d['bufferViews'][acc['bufferView']]
            fmt, size = CT[acc['componentType']]
            assert fmt == 'f', 'expected float weights'
            stride = bv.get('byteStride', size * 4)
            base = bv.get('byteOffset', 0) + acc.get('byteOffset', 0)
            for i in range(acc['count']):
                off = base + i * stride
                w = list(struct.unpack_from('<4f', buf, off))
                s = sum(w)
                if s < 1e-6:
                    w = [1.0, 0.0, 0.0, 0.0]
                    fixed += 1
                else:
                    w = [x / s for x in w]
                struct.pack_into('<4f', buf, off, *w)
            acc.pop('min', None); acc.pop('max', None)
    open(binpath, 'wb').write(buf)
    json.dump(d, open(path, 'w'))
    print(os.path.basename(path), 'fixed', fixed)
