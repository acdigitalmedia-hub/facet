import json, os, re, shutil, subprocess, sys
src = sys.argv[1]  # cute pack dir
out = sys.argv[2]
import os as _os
names = _os.environ.get("NAMES", "").split(",") if _os.environ.get("NAMES") else ["Cyclops","Deer","GreenDemon","Crab","Bee","Bat","Yeti","Ghost","YellowDragon","Alien","Cthulhu"]
manifest = {}
for n in names:
    fbx = os.path.join(src, "FBX", n + ".fbx")
    gl = os.path.join(out, n + ".gltf")
    subprocess.run(["assimp", "export", fbx, gl, "-f", "gltf2"], check=True, capture_output=True)
    tex = n + "_Texture.png"
    shutil.copy(os.path.join(src, "Textures", tex), os.path.join(out, tex))
    d = json.load(open(gl))
    d["images"] = [{"uri": tex}]
    d["samplers"] = [{"magFilter": 9729, "minFilter": 9987}]
    d["textures"] = [{"source": 0, "sampler": 0}]
    for m in d.get("materials", []):
        pbr = m.setdefault("pbrMetallicRoughness", {})
        pbr["baseColorTexture"] = {"index": 0}
        pbr["baseColorFactor"] = [1, 1, 1, 1]
        pbr["metallicFactor"] = 0.0
        pbr["roughnessFactor"] = 0.75
        m.pop("emissiveFactor", None)
    json.dump(d, open(gl, "w"))
    anims = [a.get("name", "") for a in d.get("animations", [])]
    info = subprocess.run(["assimp", "info", gl], capture_output=True, text=True).stdout
    mn = [float(x) for x in re.search(r"Minimum point\s+\(([^)]+)\)", info).group(1).split()]
    mx = [float(x) for x in re.search(r"Maximum point\s+\(([^)]+)\)", info).group(1).split()]
    def find(*keys):
        for k in keys:
            for i, a in enumerate(anims):
                if a.split("|")[-1].lower() == k:
                    return i
        return None
    manifest[n] = {"min": mn, "max": mx, "anims": anims,
                   "walk": find("walk", "flying", "idle"), "fly": find("flying", "walk", "idle"),
                   "death": find("death"), "hit": find("hitrecieve", "hitreceive")}
    print(n, [round(v,2) for v in mn], [round(v,2) for v in mx], [a.split("|")[-1] for a in anims])
mpath = os.path.join(out, "manifest.json")
old = json.load(open(mpath)) if os.path.exists(mpath) else {}
old.update(manifest)
json.dump(old, open(mpath, "w"), indent=1)
