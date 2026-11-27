//! Turns the simulation into a 3D scene: builds a map's terrain and scenery, and keeps
//! towers, creatures, projectiles and effects in sync with the game every frame.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::prelude::*;
use facet_core::color::Color as CColor;
use facet_core::game::{Event, FxKind, Game, Kind, Occ, Phase};

use crate::mesh::{self, rnd, TreeKind};
use crate::terrain::{fbm, smoothstep, Terrain, MARGIN, WATER_Y};
use crate::theme::{theme, Theme};
use crate::{CamRig, Sim};

/// Everything that belongs to the current map (despawned on map change).
#[derive(Component)]
pub struct Board;

#[derive(Component)]
pub struct Spin(pub f32);

#[derive(Component)]
pub struct Bob {
    pub base: f32,
    pub amp: f32,
    pub speed: f32,
    pub phase: f32,
}

#[derive(Component)]
pub struct Wing(pub f32);

#[derive(Component)]
pub struct Particle {
    pub vel: Vec3,
    pub life: f32,
    pub max: f32,
    pub scale: Vec3,
    pub gravity: f32,
    pub kind: Fade,
}

/// How a particle changes size over its life.
#[derive(Clone, Copy, PartialEq)]
pub enum Fade {
    /// Shrinks away (sparks, debris).
    Shrink,
    /// Swells as it fades (smoke).
    Grow,
    /// Expands outward fast (shockwaves).
    Ring,
    /// Gets thinner but keeps its height (light pillars).
    Thin,
}

#[derive(Component)]
pub struct TerrainMesh;

#[derive(Component)]
pub struct Projectile3d;

#[derive(Component)]
pub struct CreatureModel;

/// Shared meshes and materials.
#[derive(Resource)]
pub struct Gfx {
    gems: Vec<Handle<Mesh>>,
    pedestal: Handle<Mesh>,
    rim: Handle<Mesh>,
    special_base: Handle<Mesh>,
    special_crystal: Handle<Mesh>,
    shard: Handle<Mesh>,
    boulders: Vec<Handle<Mesh>>,
    standing_stone: Handle<Mesh>,
    swirl: Handle<Mesh>,
    rune_ring: Handle<Mesh>,
    pub(crate) sphere: Handle<Mesh>,
    pub(crate) cube: Handle<Mesh>,
    shock: Handle<Mesh>,
    pillar: Handle<Mesh>,
    parts: [Handle<Mesh>; 6],
    /// Vertex-colored, matte (stone, props).
    pub matte: Handle<StandardMaterial>,
    /// Terrain: vertex colors times a tiling detail texture.
    ground: Handle<StandardMaterial>,
    /// The road strip: like the ground, but with soft transparent edges.
    road: Handle<StandardMaterial>,
    metals: Vec<Handle<StandardMaterial>>,
    eye: Handle<StandardMaterial>,
    by_color: HashMap<(u32, u8), Handle<StandardMaterial>>,
}

impl Gfx {
    pub fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Gfx {
        let detail = images.add(crate::env::ground_detail());
        let gems = ["Ruby", "Sapphire", "Emerald", "Topaz", "Amethyst", "Diamond"].iter().map(|n| meshes.add(mesh::gem(n))).collect();
        let boulders = (0..6).map(|s| meshes.add(mesh::boulder(s * 13 + 1, [0.5, 0.48, 0.45]))).collect();
        let cp = mesh::creature_parts();
        let metal = |c: [f32; 3], rough: f32, glow: f32| StandardMaterial {
            base_color: Color::srgb(c[0], c[1], c[2]),
            metallic: 1.0,
            perceptual_roughness: rough,
            emissive: LinearRgba::rgb(c[0] * glow, c[1] * glow, c[2] * glow),
            ..default()
        };
        Gfx {
            gems,
            pedestal: meshes.add(mesh::pedestal()),
            rim: meshes.add(mesh::rim()),
            special_base: meshes.add(mesh::special_base()),
            special_crystal: meshes.add(mesh::special_crystal()),
            shard: meshes.add(mesh::shard()),
            boulders,
            standing_stone: meshes.add(mesh::standing_stone(4)),
            swirl: meshes.add(mesh::annulus(0.12, 0.42, 18, true)),
            rune_ring: meshes.add(mesh::annulus(0.3, 0.36, 24, true)),
            sphere: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            shock: meshes.add(mesh::annulus(0.82, 1.0, 40, false)),
            pillar: meshes.add(Cylinder::new(0.3, 1.0).mesh().resolution(12)),
            parts: [
                meshes.add(cp.body),
                meshes.add(cp.head),
                meshes.add(cp.eye),
                meshes.add(cp.wing),
                meshes.add(cp.leg),
                meshes.add(cp.horn),
            ],
            matte: mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.92, ..default() }),
            road: mats.add(StandardMaterial {
                base_color: Color::linear_rgb(1.2, 1.2, 1.2),
                base_color_texture: Some(detail.clone()),
                perceptual_roughness: 0.95,
                reflectance: 0.2,
                alpha_mode: AlphaMode::Blend,
                depth_bias: 1.0,
                ..default()
            }),
            ground: mats.add(StandardMaterial {
                base_color: Color::linear_rgb(1.2, 1.2, 1.2),
                base_color_texture: Some(detail),
                perceptual_roughness: 0.95,
                reflectance: 0.2,
                ..default()
            }),
            metals: vec![
                mats.add(metal([0.42, 0.4, 0.38], 0.8, 0.0)),
                mats.add(metal([0.72, 0.45, 0.25], 0.38, 0.0)),
                mats.add(metal([0.85, 0.87, 0.9], 0.28, 0.0)),
                mats.add(metal([1.0, 0.78, 0.32], 0.25, 0.0)),
                mats.add(metal([1.0, 0.9, 0.6], 0.2, 1.6)),
            ],
            eye: mats.add(StandardMaterial { base_color: Color::BLACK, emissive: LinearRgba::rgb(6.0, 4.5, 1.2), ..default() }),
            by_color: HashMap::new(),
        }
    }

    /// A material for a color in one of a few styles (cached).
    pub(crate) fn mat(&mut self, mats: &mut Assets<StandardMaterial>, c: CColor, style: u8, glow: f32) -> Handle<StandardMaterial> {
        let key = ((c.r * 255.0) as u32) << 16 | ((c.g * 255.0) as u32) << 8 | (c.b * 255.0) as u32;
        let gkey = (glow * 10.0) as u32;
        self.by_color
            .entry((key ^ (gkey << 24), style))
            .or_insert_with(|| {
                let base = Color::srgb(c.r, c.g, c.b);
                let l = base.to_linear();
                let m = match style {
                    // Gem: glossy with an inner glow.
                    0 => StandardMaterial {
                        // Deeper, more saturated body color; the shine comes from reflections.
                        base_color: Color::linear_rgb(l.red.powf(1.5), l.green.powf(1.5), l.blue.powf(1.5)),
                        perceptual_roughness: 0.05,
                        reflectance: 0.6,
                        emissive: LinearRgba::rgb(l.red * glow, l.green * glow, l.blue * glow),
                        ..default()
                    },
                    // Creature skin.
                    1 => StandardMaterial { base_color: base, perceptual_roughness: 0.7, ..default() },
                    // Pure glow (projectiles, sparks).
                    2 => StandardMaterial {
                        base_color: base,
                        emissive: LinearRgba::rgb(l.red * glow, l.green * glow, l.blue * glow),
                        unlit: true,
                        ..default()
                    },
                    // Faint glass (shield bubbles, shimmer of invisible enemies).
                    4 => StandardMaterial {
                        base_color: base.with_alpha(0.18),
                        emissive: LinearRgba::rgb(l.red * glow, l.green * glow, l.blue * glow),
                        alpha_mode: AlphaMode::Blend,
                        perceptual_roughness: 0.1,
                        reflectance: 0.8,
                        cull_mode: None,
                        ..default()
                    },
                    // Very faint mist (marsh fog banks).
                    5 => StandardMaterial {
                        base_color: base.with_alpha(0.028),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        cull_mode: None,
                        ..default()
                    },
                    // Translucent glow (wraiths, portals).
                    _ => StandardMaterial {
                        base_color: base.with_alpha(0.6),
                        emissive: LinearRgba::rgb(l.red * glow, l.green * glow, l.blue * glow),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    },
                };
                mats.add(m)
            })
            .clone()
    }
}

pub struct CreepParts {
    model: Option<(Entity, f32)>,
    shimmer: Option<Entity>,
    bubble: Option<(Entity, f32)>,
    shown: bool,
}

/// Per-map scene state.
#[derive(Default)]
pub struct SceneState {
    terrain: Option<Terrain>,
    /// Per cell: (signature, root entity, floating gem entity and its base size).
    towers: HashMap<usize, (u64, Entity, Option<(Entity, f32)>)>,
    creeps: HashMap<u64, Entity>,
    projectiles: Vec<Entity>,
    path_dots: Vec<Entity>,
    /// Per creature: (model, shimmer, shield bubble) entities and whether it was last shown visible.
    parts: HashMap<u64, CreepParts>,
    /// Pooled burning-ground discs.
    flames: Vec<Entity>,
    /// Smoothed live path, cached by (board rev, previewed cell).
    path_cache: Option<((u64, Option<(i32, i32)>), Vec<Vec2>)>,
}

impl SceneState {
    pub fn ground(&self, x: f32, z: f32) -> f32 {
        self.terrain.as_ref().map_or(0.0, |t| t.height(x, z))
    }
}

/// Builds terrain, lighting and scenery for the sim's current map.
#[allow(clippy::too_many_arguments)]
pub fn build_map(
    commands: &mut Commands,
    sim: &mut Sim,
    gfx: &mut Gfx,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    ambient: &mut GlobalAmbientLight,
    clear: &mut ClearColor,
    rig: &mut CamRig,
) {
    let g = &sim.game;
    let th = theme(&g.map.theme);
    let terrain = Terrain::new(&g.map, &th);
    // The dirt road follows the map's default route (no towers), so it always shows the
    // checkpoint order; the live path is drawn separately while building.
    let m = &g.map;
    let road = facet_core::path::full_route(m.w, m.h, &m.waypoints, &|x, y| m.is_wall(x, y)).unwrap_or_else(|| g.route.clone());
    let road = crate::terrain::smooth_road(m, &road);
    let tmesh = meshes.add(terrain.mesh(&road));
    commands.spawn((Mesh3d(meshes.add(terrain.road_mesh(&road))), MeshMaterial3d(gfx.road.clone()), Transform::default(), NotShadowCaster, Board));
    commands.spawn((Mesh3d(tmesh.clone()), MeshMaterial3d(gfx.ground.clone()), Transform::default(), TerrainMesh, Board));

    if terrain.water {
        let (w, h) = (g.map.w as f32, g.map.h as f32);
        let wc = th.water;
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(w + 2.0 * MARGIN, h + 2.0 * MARGIN))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgba(wc[0], wc[1], wc[2], wc[3]),
                perceptual_roughness: 0.06,
                reflectance: 0.7,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(w / 2.0, WATER_Y, h / 2.0),
            NotShadowCaster,
            Board,
        ));
    }

    // Lighting.
    let (elev, azim) = (th.sun_angle.0.to_radians(), th.sun_angle.1.to_radians());
    let center = Vec3::new(g.map.w as f32 / 2.0, 0.0, g.map.h as f32 / 2.0);
    let to_sun = Vec3::new(elev.cos() * azim.sin(), elev.sin(), elev.cos() * azim.cos());
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(th.sun[0], th.sun[1], th.sun[2]),
            illuminance: th.sun_lux,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(center + to_sun * 60.0).looking_at(center, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 3, first_cascade_far_bound: 25.0, maximum_distance: 140.0, ..default() }.build(),
        Board,
    ));
    ambient.color = Color::srgb(th.ambient[0], th.ambient[1], th.ambient[2]);
    ambient.brightness = th.ambient_brightness;
    clear.0 = Color::srgb(th.fog[0], th.fog[1], th.fog[2]);

    scenery(commands, g, &th, &terrain, gfx, meshes, mats);
    checkpoints(commands, g, &terrain, gfx, mats);

    rig.set(crate::view_preset(g.map.w as f32, g.map.h as f32, false));
    rig.target = None;
    // Debug: FACET3D_CAM="x,z,dist,pitch,yaw" (degrees) for screenshots.
    if let Ok(v) = std::env::var("FACET3D_CAM") {
        let f: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if f.len() == 5 {
            rig.focus = Vec3::new(f[0], 0.0, f[1]);
            rig.dist = f[2];
            rig.pitch = f[3].to_radians();
            rig.yaw = f[4].to_radians();
        }
    }

    sim.scene = SceneState { terrain: Some(terrain), ..default() };
}

fn scenery(commands: &mut Commands, g: &Game, th: &Theme, terrain: &Terrain, gfx: &Gfx, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) {
    let (w, h) = (g.map.w as f32, g.map.h as f32);
    let tree_meshes: Vec<(Handle<Mesh>, TreeKind)> =
        (0..8).map(|i| { let k = th.trees[i % th.trees.len()]; (meshes.add(mesh::tree(k, i as u32 * 31 + 3)), k) }).collect();
    let bush_meshes: Vec<Handle<Mesh>> = (0..4).map(|i| meshes.add(mesh::bush(th.bush, i * 17 + 5))).collect();
    let rock_meshes: Vec<Handle<Mesh>> = (0..4).map(|i| meshes.add(mesh::boulder(i * 7 + 2, th.rock))).collect();
    let seed = 41u32;
    let ember = mats.add(StandardMaterial { base_color: Color::BLACK, emissive: LinearRgba::rgb(12.0, 3.0, 0.4), ..default() });

    // Trees and rocks in the margin, thicker further from the board.
    let step = 1.1;
    let mut z = -MARGIN + 0.5;
    while z < h + MARGIN {
        let mut x = -MARGIN + 0.5;
        while x < w + MARGIN {
            let (xi, zi) = ((x * 10.0) as i32, (z * 10.0) as i32);
            let jx = x + (rnd(xi, zi, seed) - 0.5) * step;
            let jz = z + (rnd(xi, zi, seed + 1) - 0.5) * step;
            let dx = (-jx).max(jx - w).max(0.0);
            let dz = (-jz).max(jz - h).max(0.0);
            let out = (dx * dx + dz * dz).sqrt();
            let density = smoothstep(1.2, 5.0, out) * (0.25 + 0.75 * fbm(jx * 0.15, jz * 0.15, seed + 3));
            let r = rnd(xi, zi, seed + 2);
            let y = terrain.height(jx, jz);
            let yaw = rnd(xi, zi, seed + 4) * TAU;
            let scale = 0.8 + 0.6 * rnd(xi, zi, seed + 5);
            if out > 1.2 && r < density * 0.55 {
                let (m, _) = &tree_meshes[(hash_u(xi, zi) % tree_meshes.len() as u32) as usize];
                commands.spawn((
                    Mesh3d(m.clone()),
                    MeshMaterial3d(gfx.matte.clone()),
                    Transform::from_xyz(jx, y - 0.05, jz).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)),
                    Board,
                ));
            } else if out > 0.6 && r > 0.93 {
                let m = &rock_meshes[(hash_u(xi, zi) % 4) as usize];
                commands.spawn((
                    Mesh3d(m.clone()),
                    MeshMaterial3d(gfx.matte.clone()),
                    Transform::from_xyz(jx, y - 0.1, jz).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale * 1.4)),
                    Board,
                ));
            } else if out > 0.3 && r > 0.85 && !th.embers {
                let m = &bush_meshes[(hash_u(xi, zi) % 4) as usize];
                commands.spawn((
                    Mesh3d(m.clone()),
                    MeshMaterial3d(gfx.matte.clone()),
                    Transform::from_xyz(jx, y, jz).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)),
                    Board,
                ));
            } else if th.embers && r > 0.9 && out > 0.3 {
                commands.spawn((
                    Mesh3d(gfx.shard.clone()),
                    MeshMaterial3d(ember.clone()),
                    Transform::from_xyz(jx, y + 0.05, jz)
                        .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(0.5))
                        .with_scale(Vec3::splat(0.25 * scale)),
                    NotShadowCaster,
                    Board,
                ));
            }
            x += step;
        }
        z += step;
    }

    // Boulders piled along rock walls, reeds around lakes.
    for (i, wall) in g.map.walls.iter().enumerate() {
        let [wx, wy, ww, wh] = *wall;
        for cy in wy..wy + wh {
            for cx in wx..wx + ww {
                let r = rnd(cx, cy, 5 + i as u32);
                let (px, pz) = (cx as f32 + 0.5 + (r - 0.5) * 0.5, cy as f32 + 0.5 + (rnd(cx, cy, 9) - 0.5) * 0.5);
                if terrain.water {
                    let edge = cx == wx || cy == wy || cx == wx + ww - 1 || cy == wy + wh - 1;
                    if edge && r > 0.45 {
                        let (m, _) = tree_meshes.iter().find(|(_, k)| matches!(k, TreeKind::Reeds)).unwrap_or(&tree_meshes[0]);
                        commands.spawn((
                            Mesh3d(m.clone()),
                            MeshMaterial3d(gfx.matte.clone()),
                            Transform::from_xyz(px, terrain.height(px, pz).max(WATER_Y - 0.1), pz).with_scale(Vec3::splat(0.8)),
                            Board,
                        ));
                    }
                } else if r > 0.25 {
                    let m = &rock_meshes[(hash_u(cx, cy) % 4) as usize];
                    commands.spawn((
                        Mesh3d(m.clone()),
                        MeshMaterial3d(gfx.matte.clone()),
                        Transform::from_xyz(px, terrain.height(px, pz) - 0.15, pz)
                            .with_rotation(Quat::from_rotation_y(r * TAU))
                            .with_scale(Vec3::new(1.5, 1.2 + r, 1.5)),
                        Board,
                    ));
                }
            }
        }
    }
}

fn hash_u(x: i32, y: i32) -> u32 {
    mesh::hash(x, y, 1234)
}

fn checkpoints(commands: &mut Commands, g: &Game, terrain: &Terrain, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>) {
    let n = g.map.waypoints.len();
    for (i, &(x, y)) in g.map.waypoints.iter().enumerate() {
        let (px, pz) = (x as f32 + 0.5, y as f32 + 0.5);
        let base = Vec3::new(px, terrain.height(px, pz), pz);
        let (col, portal) = match i {
            0 => (CColor::new(0.65, 0.4, 1.0, 1.0), true),
            _ if i == n - 1 => (CColor::new(1.0, 0.3, 0.2, 1.0), true),
            _ => (CColor::new(1.0, 0.8, 0.4, 1.0), false),
        };
        let glow = gfx.mat(mats, col, 3, 6.0);
        let mut e = commands.spawn((Transform::from_translation(base), Visibility::default(), Board));
        e.with_children(|p| {
            // A ring of standing stones.
            for k in 0..6 {
                let a = k as f32 / 6.0 * TAU + 0.3;
                p.spawn((
                    Mesh3d(gfx.standing_stone.clone()),
                    MeshMaterial3d(gfx.matte.clone()),
                    Transform::from_xyz(a.cos() * 0.46, -0.02, a.sin() * 0.46)
                        .with_rotation(Quat::from_rotation_y(-a))
                        .with_scale(Vec3::new(1.0, if portal { 1.6 } else { 1.0 }, 1.0)),
                ));
            }
            p.spawn((
                Mesh3d(if portal { gfx.swirl.clone() } else { gfx.rune_ring.clone() }),
                MeshMaterial3d(glow.clone()),
                Transform::from_xyz(0.0, 0.04, 0.0),
                Spin(if portal { 2.5 } else { 0.4 }),
                NotShadowCaster,
            ));
            if portal {
                p.spawn((
                    Mesh3d(gfx.swirl.clone()),
                    MeshMaterial3d(glow.clone()),
                    Transform::from_xyz(0.0, 0.07, 0.0).with_scale(Vec3::splat(0.65)),
                    Spin(-3.5),
                    NotShadowCaster,
                ));
                p.spawn((
                    PointLight { color: Color::srgb(col.r, col.g, col.b), intensity: 60_000.0, range: 5.0, ..default() },
                    Transform::from_xyz(0.0, 0.6, 0.0),
                ));
            }
        });
    }
}

/// A signature of what a cell shows, to detect changes.
fn cell_sig(g: &Game, i: usize) -> u64 {
    match &g.grid[i] {
        Occ::Empty | Occ::Wall => 0,
        Occ::Rock => 1,
        Occ::Tower(t) => {
            let k = match t.kind {
                Kind::Gem(n) => n as u64,
                Kind::Special(n) => 100 + n as u64,
            };
            2 + k * 1000 + t.rank as u64 * 10 + t.fresh as u64
        }
    }
}

/// Spawns, despawns and animates everything that follows the simulation.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    commands: &mut Commands,
    sim: &mut Sim,
    gfx: &mut Gfx,
    mats: &mut Assets<StandardMaterial>,
    transforms: &mut Query<&mut Transform, Without<CreatureModel>>,
    models: &mut crate::creatures::Models,
    time: f32,
) {
    let Sim { game: g, scene, .. } = sim;
    let Some(terrain) = scene.terrain.as_ref() else { return };

    // Towers and rocks.
    for i in 0..g.grid.len() {
        let sig = cell_sig(g, i);
        let current = scene.towers.get(&i).map(|t| t.0).unwrap_or(0);
        if sig == current {
            continue;
        }
        if let Some((_, e, _)) = scene.towers.remove(&i) {
            commands.entity(e).despawn();
        }
        if sig != 0 {
            let c = g.center(i);
            let pos = Vec3::new(c.x, terrain.height(c.x, c.y), c.y);
            let (e, gem) = spawn_tower(commands, g, i, pos, gfx, mats);
            scene.towers.insert(i, (sig, e, gem));
        }
    }

    // Gems swell briefly when they fire.
    for (&i, &(_, _, gem)) in &scene.towers {
        if let (Some((ge, base)), Some(tw)) = (gem, g.tower(i))
            && let Ok(mut t) = transforms.get_mut(ge)
        {
            t.scale = Vec3::splat(base * (1.0 + 0.35 * tw.flash));
        }
    }

    // Creatures.
    let alive: std::collections::HashSet<u64> = g.creeps.iter().map(|c| c.id).collect();
    let exit = g.map.waypoints.last().map_or(Vec2::ZERO, |w| Vec2::new(w.0 as f32 + 0.5, w.1 as f32 + 0.5));
    scene.parts.retain(|id, _| alive.contains(id));
    scene.creeps.retain(|id, e| {
        if alive.contains(id) {
            return true;
        }
        // Killed creatures with a model play their death animation; ones that escaped vanish.
        let leaked = transforms.get(*e).is_ok_and(|t| Vec2::new(t.translation.x, t.translation.z).distance(exit) < 1.2);
        if leaked || !models.has_entity(*e) {
            commands.entity(*e).despawn();
        } else {
            commands.entity(*e).remove::<Board>().insert((crate::creatures::Corpse::new(), Board));
        }
        false
    });
    for c in &g.creeps {
        let ground = terrain.height(c.pos.x, c.pos.y);
        let y = if c.air { ground.max(0.0) + 1.5 } else { ground };
        let yaw = c.dir.x.atan2(c.dir.y);
        let tf = Transform::from_xyz(c.pos.x, y, c.pos.y).with_rotation(Quat::from_rotation_y(yaw));
        match scene.creeps.get(&c.id) {
            Some(&e) => {
                if let Ok(mut t) = transforms.get_mut(e) {
                    // Turn smoothly rather than snapping.
                    let rot = t.rotation.slerp(tf.rotation, 0.25);
                    *t = tf.with_rotation(rot);
                }
                if let Some(parts) = scene.parts.get_mut(&c.id) {
                    // Unrevealed invisible enemies show only as a faint shimmer.
                    let shown = c.visible();
                    if shown != parts.shown {
                        parts.shown = shown;
                        if let Some((m, _)) = parts.model {
                            commands.entity(m).insert(if shown { Visibility::Inherited } else { Visibility::Hidden });
                        }
                        if let Some(sh) = parts.shimmer {
                            commands.entity(sh).insert(if shown { Visibility::Hidden } else { Visibility::Inherited });
                        }
                    }
                    // Hit reaction: a quick squash and a nudge backwards.
                    if let Some((m, base)) = parts.model
                        && let Ok(mut mt) = transforms.get_mut(m)
                    {
                        let k = (c.flash / 0.08).clamp(0.0, 1.0);
                        mt.scale = Vec3::new(base * (1.0 + 0.12 * k), base * (1.0 - 0.15 * k), base * (1.0 + 0.12 * k));
                        mt.translation = Vec3::new(0.0, 0.0, -0.08 * k);
                    }
                    if let Some((b, r)) = parts.bubble
                        && let Ok(mut t) = transforms.get_mut(b)
                    {
                        let f = if c.max_shield > 0.0 { c.shield / c.max_shield } else { 0.0 };
                        let wobble = 1.0 + 0.04 * (time * 6.0 + c.id as f32).sin();
                        t.scale = Vec3::splat(if f > 0.01 { r * (0.75 + 0.25 * f) * wobble } else { 0.0 });
                    }
                }
            }
            None => {
                let k = &g.bal.waves.kinds[c.kind];
                let e = if models.has(&k.look) {
                    let root = commands.spawn((tf, Visibility::default(), Board)).id();
                    let height = k.size * 3.3;
                    let model = crate::creatures::spawn(commands, models, root, &k.look, height, c.air, 0.9 + 0.5 * k.speed_mult);
                    let mut parts = CreepParts { model, shimmer: None, bubble: None, shown: true };
                    if k.invisible {
                        let m = gfx.mat(mats, CColor::new(0.75, 0.8, 1.0, 1.0), 4, 0.3);
                        let sh = commands
                            .spawn((
                                Mesh3d(gfx.sphere.clone()),
                                MeshMaterial3d(m),
                                Transform::from_xyz(0.0, height * 0.45, 0.0).with_scale(Vec3::new(height * 0.4, height * 0.5, height * 0.4)),
                                Visibility::Hidden,
                                NotShadowCaster,
                            ))
                            .id();
                        commands.entity(root).add_child(sh);
                        parts.shimmer = Some(sh);
                    }
                    if k.shield > 0.0 {
                        let m = gfx.mat(mats, CColor::new(0.45, 0.7, 1.0, 1.0), 4, 0.5);
                        let r = height * 0.55;
                        let b = commands
                            .spawn((Mesh3d(gfx.sphere.clone()), MeshMaterial3d(m), Transform::from_xyz(0.0, height * 0.45, 0.0).with_scale(Vec3::splat(r)), NotShadowCaster))
                            .id();
                        commands.entity(root).add_child(b);
                        parts.bubble = Some((b, r));
                    }
                    scene.parts.insert(c.id, parts);
                    models.mark(root);
                    root
                } else {
                    spawn_creature(commands, g, c.kind, tf, gfx, mats)
                };
                scene.creeps.insert(c.id, e);
            }
        }
    }

    // Burning ground (pooled flat discs that flicker), with embers rising.
    while scene.flames.len() < g.patches.len() {
        let m = gfx.mat(mats, CColor::new(1.0, 0.45, 0.1, 1.0), 3, 6.0);
        let e = commands.spawn((Mesh3d(gfx.sphere.clone()), MeshMaterial3d(m), Transform::default(), Visibility::Hidden, NotShadowCaster, Board)).id();
        scene.flames.push(e);
    }
    for (k, &e) in scene.flames.iter().enumerate() {
        match g.patches.get(k) {
            Some(p) => {
                if let Ok(mut t) = transforms.get_mut(e) {
                    let fade = (1.0 - p.t / 3.0).clamp(0.0, 1.0);
                    let flicker = 0.9 + 0.1 * (time * 17.0 + k as f32 * 2.1).sin();
                    let r = facet_core::game::BURN_RADIUS * (0.5 + 0.5 * fade) * flicker;
                    *t = Transform::from_xyz(p.pos.x, terrain.height(p.pos.x, p.pos.y) + 0.04, p.pos.y).with_scale(Vec3::new(r, 0.03, r));
                }
                commands.entity(e).insert(Visibility::Inherited);
                if rnd(k as i32, (time * 60.0) as i32, 5) < 0.12 {
                    let a = rnd(k as i32, (time * 60.0) as i32, 6) * TAU;
                    let at = Vec3::new(p.pos.x + a.cos() * 0.4, terrain.height(p.pos.x, p.pos.y) + 0.1, p.pos.y + a.sin() * 0.4);
                    let m = gfx.mat(mats, CColor::new(1.0, 0.6, 0.15, 1.0), 2, 8.0);
                    commands.spawn((
                        Mesh3d(gfx.cube.clone()),
                        MeshMaterial3d(m),
                        Transform::from_translation(at).with_scale(Vec3::splat(0.04)),
                        Particle { vel: Vec3::new(0.0, 1.2, 0.0), life: 0.7, max: 0.7, scale: Vec3::splat(0.05), gravity: -0.5, kind: Fade::Shrink },
                        NotShadowCaster,
                        Board,
                    ));
                }
            }
            None => {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }

    // Projectiles (pooled).
    while scene.projectiles.len() < g.projectiles.len() {
        let e = commands
            .spawn((Mesh3d(gfx.sphere.clone()), MeshMaterial3d(gfx.eye.clone()), Transform::default(), NotShadowCaster, Projectile3d, Board))
            .id();
        scene.projectiles.push(e);
    }
    let mut trails = 0;
    for (k, &e) in scene.projectiles.iter().enumerate() {
        if let Some(p) = g.projectiles.get(k) {
            let ground = terrain.height(p.pos.x, p.pos.y);
            let air = g.creeps.iter().find(|c| c.id == p.target).is_some_and(|c| c.air);
            let y = ground + if air { 1.4 } else { 0.6 };
            let style = g.tower(p.src).map_or(Attack::Orb, |t| attack_style(g.def(t.kind)));
            let r = 0.07 + p.size * 0.04;
            let dir = Vec3::new(p.vel.x, 0.0, p.vel.y).normalize_or(Vec3::Z);
            let at = Vec3::new(p.pos.x, y, p.pos.y);
            let face = Quat::from_rotation_arc(Vec3::Z, dir);
            let (mesh, tf, glow) = match style {
                Attack::Fire => (gfx.sphere.clone(), Transform::from_translation(at).with_rotation(face).with_scale(Vec3::new(r * 1.4, r * 1.4, r * 2.2)), 10.0),
                Attack::Frost => {
                    let spin = Quat::from_rotation_z(time * 12.0);
                    (gfx.shard.clone(), Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir) * spin).with_scale(Vec3::splat(r * 2.6)), 6.0)
                }
                Attack::Poison => {
                    let wob = 1.0 + 0.2 * (time * 20.0 + k as f32).sin();
                    (gfx.sphere.clone(), Transform::from_translation(at).with_scale(Vec3::new(r * 1.3 * wob, r * 1.3 / wob, r * 1.3 * wob)), 4.0)
                }
                Attack::Spark => (gfx.sphere.clone(), Transform::from_translation(at).with_rotation(face).with_scale(Vec3::new(r * 0.6, r * 0.6, r * 5.0)), 14.0),
                Attack::Lance => (gfx.sphere.clone(), Transform::from_translation(at).with_rotation(face).with_scale(Vec3::new(r * 0.5, r * 0.5, r * 9.0)), 12.0),
                Attack::Shard => {
                    let spin = Quat::from_rotation_y(time * 15.0);
                    (gfx.shard.clone(), Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir) * spin).with_scale(Vec3::splat(r * 2.2)), 9.0)
                }
                Attack::Orb => (gfx.sphere.clone(), Transform::from_translation(at).with_rotation(face).with_scale(Vec3::new(r, r, r * 3.2)), 8.0),
            };
            if let Ok(mut t) = transforms.get_mut(e) {
                *t = tf;
            }
            let m = gfx.mat(mats, p.color, 2, glow);
            commands.entity(e).insert((Mesh3d(mesh), MeshMaterial3d(m), Visibility::Inherited));

            // Trails.
            let seed = (time * 60.0) as i32;
            let roll = rnd(k as i32, seed, 31);
            if trails < 80 {
                let trail = match style {
                    Attack::Fire if roll < 0.7 => Some((CColor::new(1.0, 0.55 + 0.3 * roll, 0.15, 1.0), Vec3::new(0.0, 0.8, 0.0), -1.0, 0.06, 0.35)),
                    Attack::Frost if roll < 0.45 => Some((CColor::new(0.8, 0.92, 1.0, 1.0), Vec3::ZERO, 0.5, 0.035, 0.4)),
                    Attack::Poison if roll < 0.5 => Some((CColor::new(0.35, 0.95, 0.35, 1.0), Vec3::ZERO, 6.0, 0.045, 0.45)),
                    Attack::Shard if roll < 0.3 => Some((CColor::new(1.0, 1.0, 1.0, 1.0), Vec3::ZERO, 0.0, 0.03, 0.3)),
                    _ => None,
                };
                if let Some((col, vel, gravity, size, life)) = trail {
                    trails += 1;
                    let m = gfx.mat(mats, col, 2, 6.0);
                    let jitter = Vec3::new(rnd(k as i32, seed, 32) - 0.5, rnd(k as i32, seed, 33) - 0.5, rnd(k as i32, seed, 34) - 0.5) * 0.08;
                    commands.spawn((
                        Mesh3d(gfx.cube.clone()),
                        MeshMaterial3d(m),
                        Transform::from_translation(at + jitter).with_scale(Vec3::splat(size)),
                        Particle { vel, life, max: life, scale: Vec3::splat(size), gravity, kind: Fade::Shrink },
                        NotShadowCaster,
                        Board,
                    ));
                }
            }
        } else {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }

    // Events: bursts of particles, shockwaves, smoke and light.
    let events: Vec<Event> = std::mem::take(&mut g.events);
    let ground_at = |p: facet_core::game::GVec2| Vec3::new(p.x, terrain.height(p.x, p.y), p.y);
    let mut impacts = 0;
    for ev in events {
        match ev {
            Event::Impact { pos, src } => {
                impacts += 1;
                if impacts > 24 {
                    continue;
                }
                let Some(t) = g.tower(src) else { continue };
                let col = g.color(t.kind);
                let air = g.creeps.iter().filter(|c| c.air).any(|c| c.pos.distance(pos) < 0.3);
                let at = ground_at(pos) + Vec3::Y * if air { 1.4 } else { 0.5 };
                match attack_style(g.def(t.kind)) {
                    Attack::Fire => {
                        let m = gfx.mat(mats, CColor::new(1.0, 0.6, 0.15, 1.0), 2, 10.0);
                        burst(commands, gfx, m, at, 6, 2.0, 0.06, time);
                    }
                    Attack::Frost => {
                        let m = gfx.mat(mats, CColor::new(0.8, 0.93, 1.0, 1.0), 2, 6.0);
                        burst(commands, gfx, m, at, 6, 2.2, 0.045, time);
                        shockwave(commands, gfx, mats, at - Vec3::Y * 0.4, CColor::new(0.6, 0.85, 1.0, 1.0), 0.55, 0.25);
                    }
                    Attack::Poison => {
                        let m = gfx.mat(mats, CColor::new(0.35, 0.95, 0.35, 1.0), 2, 4.0);
                        burst(commands, gfx, m, at, 7, 1.4, 0.05, time);
                    }
                    Attack::Spark => {
                        let m = gfx.mat(mats, CColor::new(1.0, 0.95, 0.5, 1.0), 2, 14.0);
                        burst(commands, gfx, m, at, 4, 4.0, 0.03, time);
                    }
                    Attack::Lance => shockwave(commands, gfx, mats, at - Vec3::Y * 0.3, col, 0.45, 0.2),
                    Attack::Shard | Attack::Orb => {
                        let m = gfx.mat(mats, col, 2, 9.0);
                        burst(commands, gfx, m, at, 4, 2.5, 0.035, time);
                    }
                }
            }
            Event::Death { pos, color, boss } => {
                let base = ground_at(pos) + Vec3::Y * 0.3;
                let m = gfx.mat(mats, color, 2, 3.0);
                burst(commands, gfx, m, base, if boss { 40 } else { 10 }, if boss { 4.0 } else { 2.5 }, 0.09, time);
                smoke(commands, gfx, mats, base, if boss { 10 } else { 3 }, if boss { 0.6 } else { 0.25 }, time);
                if boss {
                    shockwave(commands, gfx, mats, ground_at(pos), color, 3.5, 0.7);
                }
            }
            Event::Splash { pos, color, radius } => {
                shockwave(commands, gfx, mats, ground_at(pos), color, radius, 0.35);
            }
            Event::Place { pos, .. } | Event::RockRemoved { pos } => {
                let base = ground_at(pos) + Vec3::Y * 0.1;
                let dust = gfx.mat(mats, CColor::new(0.6, 0.55, 0.45, 1.0), 1, 0.0);
                burst(commands, gfx, dust, base, 8, 1.5, 0.08, time);
                smoke(commands, gfx, mats, base, 2, 0.2, time);
            }
            Event::Combine { pos, color } | Event::Forge { pos, color } => {
                let big = matches!(ev, Event::Forge { .. });
                let base = ground_at(pos);
                let m = gfx.mat(mats, color, 2, 10.0);
                burst(commands, gfx, m, base + Vec3::Y * 0.6, if big { 40 } else { 20 }, if big { 4.0 } else { 3.0 }, 0.06, time);
                pillar(commands, gfx, mats, base, color, if big { 1.2 } else { 0.8 });
                shockwave(commands, gfx, mats, base, color, if big { 2.5 } else { 1.5 }, 0.5);
            }
            Event::Keep { pos, color } | Event::Upgrade { pos: Some(pos), color } => {
                let base = ground_at(pos);
                let m = gfx.mat(mats, color, 2, 10.0);
                burst(commands, gfx, m, base + Vec3::Y * 0.6, 14, 3.0, 0.05, time);
                pillar(commands, gfx, mats, base, color, 0.5);
            }
            Event::Crit { pos, color } => {
                let m = gfx.mat(mats, color, 2, 10.0);
                burst(commands, gfx, m, ground_at(pos) + Vec3::Y * 0.5, 6, 3.0, 0.04, time);
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn burst(commands: &mut Commands, gfx: &Gfx, mat: Handle<StandardMaterial>, at: Vec3, n: usize, speed: f32, size: f32, time: f32) {
    let s = (time * 1000.0) as i32;
    for k in 0..n {
        let a = rnd(k as i32, s, 1) * TAU;
        let up = 0.4 + rnd(k as i32, s, 2) * 1.2;
        let v = Vec3::new(a.cos(), up, a.sin()) * speed * (0.5 + rnd(k as i32, s, 3));
        let life = 0.5 + rnd(k as i32, s, 4) * 0.4;
        commands.spawn((
            Mesh3d(gfx.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)).with_rotation(Quat::from_rotation_y(a)),
            Particle { vel: v, life, max: life, scale: Vec3::splat(size), gravity: 7.0, kind: Fade::Shrink },
            NotShadowCaster,
            Board,
        ));
    }
}

/// How a tower's shots look, from its strongest trait.
#[derive(Clone, Copy, PartialEq)]
enum Attack {
    Fire,
    Frost,
    Poison,
    Spark,
    Lance,
    Shard,
    Orb,
}

fn attack_style(d: &facet_core::data::TowerDef) -> Attack {
    if d.burn > 0.0 || d.splash > 0.0 {
        Attack::Fire
    } else if d.poison > 0.0 {
        Attack::Poison
    } else if d.chain > 0.0 {
        Attack::Spark
    } else if d.slow > 0.0 {
        Attack::Frost
    } else if d.pierce || d.range >= 6.0 {
        Attack::Lance
    } else if d.crit > 0.0 || d.aura > 0.0 {
        Attack::Shard
    } else {
        Attack::Orb
    }
}

/// An expanding glowing ring on the ground.
fn shockwave(commands: &mut Commands, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>, at: Vec3, color: CColor, radius: f32, life: f32) {
    let m = gfx.mat(mats, color, 3, 4.0);
    commands.spawn((
        Mesh3d(gfx.shock.clone()),
        MeshMaterial3d(m),
        Transform::from_translation(at + Vec3::Y * 0.12).with_scale(Vec3::new(radius, 1.0, radius) * 0.25),
        Particle { vel: Vec3::ZERO, life, max: life, scale: Vec3::new(radius, 1.0, radius), gravity: 0.0, kind: Fade::Ring },
        NotShadowCaster,
        Board,
    ));
}

/// A column of light rising from a tower (combines, forges, keeps).
fn pillar(commands: &mut Commands, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>, at: Vec3, color: CColor, life: f32) {
    let m = gfx.mat(mats, color, 3, 6.0);
    let h = 5.0;
    commands.spawn((
        Mesh3d(gfx.pillar.clone()),
        MeshMaterial3d(m),
        Transform::from_translation(at + Vec3::Y * h / 2.0).with_scale(Vec3::new(1.0, h, 1.0)),
        Particle { vel: Vec3::ZERO, life, max: life, scale: Vec3::new(1.0, h, 1.0), gravity: 0.0, kind: Fade::Thin },
        NotShadowCaster,
        Board,
    ));
}

/// Soft grey puffs that drift up and swell.
fn smoke(commands: &mut Commands, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>, at: Vec3, n: usize, size: f32, time: f32) {
    let m = gfx.mat(mats, CColor::new(0.55, 0.52, 0.5, 1.0), 3, 0.15);
    let s = (time * 997.0) as i32;
    for k in 0..n {
        let a = rnd(k as i32, s, 7) * TAU;
        let v = Vec3::new(a.cos() * 0.3, 0.6 + rnd(k as i32, s, 8) * 0.5, a.sin() * 0.3);
        let life = 0.7 + rnd(k as i32, s, 9) * 0.5;
        commands.spawn((
            Mesh3d(gfx.sphere.clone()),
            MeshMaterial3d(m.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            Particle { vel: v, life, max: life, scale: Vec3::splat(size), gravity: -0.3, kind: Fade::Grow },
            NotShadowCaster,
            Board,
        ));
    }
}

fn tier_glow(tier: usize) -> f32 {
    [0.5, 0.9, 1.6, 2.8, 5.0][tier.min(4)]
}

fn spawn_tower(commands: &mut Commands, g: &Game, i: usize, pos: Vec3, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>) -> (Entity, Option<(Entity, f32)>) {
    let (cx, cy) = g.cell_of(i);
    match &g.grid[i] {
        Occ::Rock => {
            let m = gfx.boulders[(mesh::hash(cx, cy, 3) % gfx.boulders.len() as u32) as usize].clone();
            let yaw = rnd(cx, cy, 8) * TAU;
            let s = 0.95 + 0.25 * rnd(cx, cy, 9);
            let e = commands
                .spawn((
                    Mesh3d(m),
                    MeshMaterial3d(gfx.matte.clone()),
                    Transform::from_translation(pos - Vec3::Y * 0.04).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(s)),
                    Board,
                ))
                .id();
            (e, None)
        }
        Occ::Tower(t) => {
            let col = g.color(t.kind);
            let phase = rnd(cx, cy, 2) * TAU;
            let mut root = commands.spawn((Transform::from_translation(pos), Visibility::default(), Board));
            let mut gem_entity = None;
            match t.kind {
                Kind::Gem(n) => {
                    let tier = t.rank;
                    let gem_mat = gfx.mat(mats, col, 0, tier_glow(tier));
                    let size = 0.24 + 0.035 * tier as f32;
                    let gem_mesh = gfx.gems[n.min(gfx.gems.len() - 1)].clone();
                    let rim = gfx.metals[tier.min(4)].clone();
                    root.with_children(|p| {
                        p.spawn((Mesh3d(gfx.pedestal.clone()), MeshMaterial3d(gfx.matte.clone()), Transform::default()));
                        p.spawn((Mesh3d(gfx.rim.clone()), MeshMaterial3d(rim), Transform::default()));
                        let ge = p.spawn((
                            Mesh3d(gem_mesh),
                            MeshMaterial3d(gem_mat),
                            Transform::from_xyz(0.0, 0.62, 0.0).with_scale(Vec3::splat(size)).with_rotation(Quat::from_rotation_x(0.25)),
                            Spin(0.6),
                            Bob { base: 0.6 + size * 0.4, amp: 0.04, speed: 1.6, phase },
                        )).id();
                        gem_entity = Some((ge, size));
                        if tier >= 1 {
                            p.spawn((
                                PointLight {
                                    color: Color::srgb(col.r, col.g, col.b),
                                    intensity: 4_000.0 * (tier * tier) as f32 + 6_000.0,
                                    range: 1.5 + tier as f32 * 0.6,
                                    ..default()
                                },
                                Transform::from_xyz(0.0, 0.75, 0.0),
                            ));
                        }
                    });
                }
                Kind::Special(sp) => {
                    let level = t.rank;
                    let ascended = g.bal.specials[sp].ascended;
                    let crystal = gfx.mat(mats, col, 0, 2.0 + level as f32 * 1.5 + if ascended { 3.0 } else { 0.0 });
                    let rune = gfx.mat(mats, col, 3, 5.0);
                    root.with_children(|p| {
                        if ascended {
                            // Ascended specials: a larger, higher crystal crowned by a halo.
                            p.spawn((
                                Mesh3d(gfx.special_crystal.clone()),
                                MeshMaterial3d(crystal.clone()),
                                Transform::from_xyz(0.0, 1.45, 0.0).with_scale(Vec3::splat(0.3)).with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
                                Spin(-1.4),
                                Bob { base: 1.45, amp: 0.05, speed: 1.5, phase: phase + 1.0 },
                            ));
                            p.spawn((
                                Mesh3d(gfx.rune_ring.clone()),
                                MeshMaterial3d(rune.clone()),
                                Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::splat(0.9)),
                                Spin(1.2),
                                NotShadowCaster,
                            ));
                            p.spawn((
                                Mesh3d(gfx.rune_ring.clone()),
                                MeshMaterial3d(rune.clone()),
                                Transform::from_xyz(0.0, 0.47, 0.0).with_scale(Vec3::splat(1.6)),
                                Spin(0.5),
                                NotShadowCaster,
                            ));
                        }
                        p.spawn((Mesh3d(gfx.special_base.clone()), MeshMaterial3d(gfx.matte.clone()), Transform::default()));
                        p.spawn((Mesh3d(gfx.rim.clone()), MeshMaterial3d(gfx.metals[3].clone()), Transform::from_xyz(0.0, 0.26, 0.0).with_scale(Vec3::new(0.95, 1.0, 0.95))));
                        p.spawn((
                            Mesh3d(gfx.special_crystal.clone()),
                            MeshMaterial3d(crystal.clone()),
                            Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::splat(0.36)),
                            Spin(0.9),
                            Bob { base: 1.0, amp: 0.06, speed: 1.2, phase },
                        ));
                        p.spawn((
                            Mesh3d(gfx.rune_ring.clone()),
                            MeshMaterial3d(rune),
                            Transform::from_xyz(0.0, 0.47, 0.0).with_scale(Vec3::splat(1.15)),
                            Spin(-0.7),
                            NotShadowCaster,
                        ));
                        // One orbiting shard per level, plus two.
                        p.spawn((Transform::from_xyz(0.0, 1.0, 0.0), Visibility::default(), Spin(1.8))).with_children(|o| {
                            let n = 2 + level;
                            for k in 0..n {
                                let a = k as f32 / n as f32 * TAU;
                                o.spawn((
                                    Mesh3d(gfx.shard.clone()),
                                    MeshMaterial3d(crystal.clone()),
                                    Transform::from_xyz(a.cos() * 0.45, 0.0, a.sin() * 0.45).with_scale(Vec3::splat(0.28)),
                                ));
                            }
                        });
                        p.spawn((
                            PointLight { color: Color::srgb(col.r, col.g, col.b), intensity: 40_000.0 + 20_000.0 * level as f32, range: 4.0, ..default() },
                            Transform::from_xyz(0.0, 1.0, 0.0),
                        ));
                    });
                }
            }
            (root.id(), gem_entity)
        }
        _ => (commands.spawn((Transform::default(), Board)).id(), None),
    }
}

/// Placeholder creatures assembled from a few shared parts. Real models come later.
fn spawn_creature(commands: &mut Commands, g: &Game, kind: usize, tf: Transform, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>) -> Entity {
    let k = &g.bal.waves.kinds[kind];
    let col = facet_core::game::rgb(k.color);
    let skin = if k.look == "wraith" { gfx.mat(mats, col, 3, 1.5) } else { gfx.mat(mats, col, 1, 0.0) };
    let dark = gfx.mat(mats, CColor::new(col.r * 0.45, col.g * 0.45, col.b * 0.45, 1.0), 1, 0.0);
    let [body, head, eye, wing, leg, horn] = gfx.parts.clone();
    let scale = k.size * 2.2;
    // (body scale, head offset, head scale, legs, horns, wings)
    let (bs, ho, hs, legs, horns, wings): (Vec3, Vec3, f32, u32, bool, bool) = match k.look.as_str() {
        "beetle" => (Vec3::new(1.05, 0.6, 1.0), Vec3::new(0.0, 0.05, 0.55), 0.8, 6, false, false),
        "runner" => (Vec3::new(0.7, 0.75, 1.1), Vec3::new(0.0, 0.3, 0.6), 0.85, 4, false, false),
        "brute" => (Vec3::new(1.2, 1.05, 1.05), Vec3::new(0.0, 0.25, 0.55), 0.95, 4, true, false),
        "armored" => (Vec3::new(1.15, 0.8, 1.15), Vec3::new(0.0, 0.05, 0.6), 0.8, 4, true, false),
        "swarm" => (Vec3::new(0.8, 0.7, 0.9), Vec3::new(0.0, 0.15, 0.45), 0.8, 0, false, true),
        "bat" => (Vec3::new(0.75, 0.7, 0.8), Vec3::new(0.0, 0.2, 0.4), 0.85, 0, false, true),
        "golem" => (Vec3::new(1.2, 1.3, 1.0), Vec3::new(0.0, 0.45, 0.3), 0.8, 4, true, false),
        "wraith" => (Vec3::new(0.8, 1.0, 1.4), Vec3::new(0.0, 0.3, 0.6), 0.85, 0, true, false),
        "hydra" => (Vec3::new(1.15, 0.85, 1.25), Vec3::new(0.0, 0.5, 0.55), 0.7, 4, false, false),
        "hydraling" => (Vec3::new(0.8, 0.7, 1.0), Vec3::new(0.0, 0.25, 0.5), 0.8, 4, false, false),
        "leviathan" => (Vec3::new(0.95, 0.75, 1.9), Vec3::new(0.0, 0.2, 1.05), 0.95, 0, true, false),
        _ => (Vec3::ONE, Vec3::new(0.0, 0.2, 0.55), 0.8, 4, false, false),
    };
    let lift = if legs > 0 { 0.3 } else { 0.35 };
    let phase = (kind as f32 * 1.3 + tf.translation.x) % TAU;
    let mut root = commands.spawn((tf, Visibility::default(), Board));
    root.with_children(|p| {
        p.spawn((
            Transform::from_scale(Vec3::splat(scale)),
            Visibility::default(),
            CreatureModel,
            Bob { base: 0.0, amp: if wings { 0.12 } else { 0.05 }, speed: if wings { 5.0 } else { 9.0 }, phase },
        ))
        .with_children(|m| {
            m.spawn((Mesh3d(body.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, lift + 0.2, 0.0).with_scale(bs)));
            let heads: &[f32] = if k.look == "hydra" { &[-0.45, 0.0, 0.45] } else { &[0.0] };
            for &hx in heads {
                let hp = Vec3::new(hx, lift + 0.2, 0.0) + ho * Vec3::new(1.0, 1.0, bs.z);
                m.spawn((Mesh3d(head.clone()), MeshMaterial3d(skin.clone()), Transform::from_translation(hp).with_scale(Vec3::splat(hs))));
                for s in [-1.0, 1.0] {
                    m.spawn((
                        Mesh3d(eye.clone()),
                        MeshMaterial3d(gfx.eye.clone()),
                        Transform::from_translation(hp + Vec3::new(s * 0.12 * hs, 0.06 * hs, 0.24 * hs)),
                        NotShadowCaster,
                    ));
                    if horns {
                        m.spawn((
                            Mesh3d(horn.clone()),
                            MeshMaterial3d(dark.clone()),
                            Transform::from_translation(hp + Vec3::new(s * 0.17 * hs, 0.18 * hs, 0.0)).with_rotation(Quat::from_rotation_z(-s * 0.5)),
                        ));
                    }
                }
            }
            for l in 0..legs {
                let side = if l % 2 == 0 { -1.0 } else { 1.0 };
                let row = (l / 2) as f32 / ((legs / 2).max(2) - 1) as f32 - 0.5;
                m.spawn((
                    Mesh3d(leg.clone()),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_xyz(side * 0.38 * bs.x, lift + 0.05, row * 0.7 * bs.z).with_rotation(Quat::from_rotation_z(side * 0.4)),
                ));
            }
            if wings {
                for s in [-1.0f32, 1.0] {
                    m.spawn((
                        Transform::from_xyz(s * 0.3, lift + 0.3, 0.0).with_scale(Vec3::new(s, 1.0, 1.0) * 1.3),
                        Visibility::default(),
                        Wing(s),
                    ))
                    .with_children(|w| {
                        w.spawn((Mesh3d(wing.clone()), MeshMaterial3d(dark.clone()), Transform::default()));
                    });
                }
            }
        });
    });
    root.id()
}

/// Gentle idle motion: spinning gems, bobbing, flapping wings, particles.
pub fn animate(
    mut commands: Commands,
    time: Res<Time>,
    mut spin: Query<(&Spin, &mut Transform), (Without<Bob>, Without<Wing>, Without<Particle>)>,
    mut bob: Query<(&Bob, &mut Transform, Option<&Spin>), (Without<Wing>, Without<Particle>)>,
    mut wings: Query<(&Wing, &mut Transform), Without<Particle>>,
    mut parts: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    for (s, mut tf) in &mut spin {
        tf.rotate_y(s.0 * dt);
    }
    for (b, mut tf, s) in &mut bob {
        tf.translation.y = b.base + (t * b.speed + b.phase).sin() * b.amp;
        if let Some(s) = s {
            tf.rotate_y(s.0 * dt);
        }
    }
    for (w, mut tf) in &mut wings {
        tf.rotation = Quat::from_rotation_z(w.0 * (t * 14.0).sin() * 0.7);
    }
    for (e, mut p, mut tf) in &mut parts {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        tf.translation += p.vel * dt;
        let k = p.life / p.max;
        match p.kind {
            Fade::Shrink => {
                tf.rotate_y(dt * 6.0);
                tf.scale = p.scale * k.sqrt();
            }
            Fade::Grow => tf.scale = p.scale * (1.0 + 1.8 * (1.0 - k)) * k.powf(0.3),
            Fade::Ring => tf.scale = p.scale * (0.25 + 0.75 * (1.0 - k * k)),
            Fade::Thin => {
                tf.rotate_y(dt * 2.0);
                tf.scale = Vec3::new(p.scale.x * k, p.scale.y * (1.0 + 0.3 * (1.0 - k)), p.scale.z * k);
            }
        }
    }
}

/// Lines drawn each frame: health bars, lightning, splash rings, the build grid, the path
/// preview, selection and recipe hints.
pub fn overlays(sim: &Sim, st: &crate::ui::UiState, gizmos: &mut Gizmos) {
    let g = &sim.game;
    let ground = |x: f32, z: f32| sim.scene.ground(x, z);
    let up_rot = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    let playing = sim.screen == crate::Screen::Playing;
    let t = sim.time;
    let at = |c: (i32, i32), lift: f32| {
        let (x, z) = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        Vec3::new(x, ground(x, z) + lift, z)
    };

    for c in &g.creeps {
        let k = &g.bal.waves.kinds[c.kind];
        let base = Vec3::new(c.pos.x, ground(c.pos.x, c.pos.y) + if c.air { 1.5 } else { 0.0 }, c.pos.y);
        if !c.visible() {
            continue;
        }
        // Menders: a slow green pulse showing their healing reach.
        if c.heal > 0.0 {
            let k2 = (t * 0.8 + c.id as f32 * 0.37).fract();
            gizmos.circle(
                Isometry3d::new(base + Vec3::Y * 0.08, up_rot),
                facet_core::game::HEAL_RANGE * (0.3 + 0.7 * k2),
                Color::srgba(0.4, 1.0, 0.5, 0.6 * (1.0 - k2)),
            );
        }
        // Warded: a golden ring of protection.
        if c.immune {
            gizmos.circle(Isometry3d::new(base + Vec3::Y * 0.06, up_rot), k.size * 1.5, Color::srgba(1.0, 0.85, 0.3, 0.85));
        }
        // Sprinting: speed streaks behind.
        if c.sprinting() {
            let back = -Vec3::new(c.dir.x, 0.0, c.dir.y).normalize_or_zero();
            let side = Vec3::new(-back.z, 0.0, back.x);
            for o in [-0.18f32, 0.0, 0.18] {
                let from = base + Vec3::Y * 0.3 + side * o + back * 0.35;
                gizmos.line(from, from + back * 0.6, Color::srgba(1.0, 0.9, 0.7, 0.7));
            }
        }
        // Status rings: frost (slowed) and poison.
        if c.slow_t > 0.0 && c.slow > 0.0 {
            gizmos.circle(Isometry3d::new(base + Vec3::Y * 0.05, up_rot), k.size * 1.6, Color::srgba(0.5, 0.85, 1.0, 0.8));
        }
        if c.poison_t > 0.0 {
            let a = 0.5 + 0.3 * (t * 4.0).sin();
            gizmos.circle(Isometry3d::new(base + Vec3::Y * 0.1, up_rot), k.size * 1.9, Color::srgba(0.3, 1.0, 0.45, a));
        }
    }

    for fx in &g.fx {
        let a = (1.0 - fx.t / fx.life).clamp(0.0, 1.0);
        let col = LinearRgba::rgb(fx.color.r * 4.0 * a, fx.color.g * 4.0 * a, fx.color.b * 4.0 * a);
        match &fx.kind {
            FxKind::Beam(p, q) => {
                // Jagged lightning.
                let (a3, b3) = (Vec3::new(p.x, ground(p.x, p.y) + 0.7, p.y), Vec3::new(q.x, ground(q.x, q.y) + 0.5, q.y));
                let mut prev = a3;
                let seg = 6;
                for s in 1..=seg {
                    let tt = s as f32 / seg as f32;
                    let mut pt = a3.lerp(b3, tt);
                    if s < seg {
                        let j = (fx.t * 60.0) as i32;
                        pt += Vec3::new(rnd(s, j, 1) - 0.5, rnd(s, j, 2) - 0.5, rnd(s, j, 3) - 0.5) * 0.35;
                    }
                    gizmos.line(prev, pt, col);
                    prev = pt;
                }
            }
            FxKind::Ring(p, r) => {
                let rr = r * (0.4 + 0.6 * (fx.t / fx.life).clamp(0.0, 1.0));
                gizmos.circle(Isometry3d::new(Vec3::new(p.x, ground(p.x, p.y) + 0.15, p.y), up_rot), rr, col);
            }
            FxKind::Text(..) => {}
        }
    }

    // Fresh gems pulse.
    let pulse = 0.5 + 0.5 * (t * 4.0).sin();
    for &i in &g.placed {
        gizmos.circle(Isometry3d::new(at(g.cell_of(i), 0.06), up_rot), 0.47, Color::srgba(1.0, 0.9, 0.5, 0.4 + 0.5 * pulse));
    }
    if !playing {
        return;
    }

    // Grid while building.
    if matches!(g.phase, Phase::Build | Phase::Choose) {
        let (w, h) = (g.map.w as f32, g.map.h as f32);
        let col = Color::srgba(0.0, 0.0, 0.0, 0.18);
        for x in 0..=g.map.w {
            gizmos.line(Vec3::new(x as f32, 0.07, 0.0), Vec3::new(x as f32, 0.07, h), col);
        }
        for z in 0..=g.map.h {
            gizmos.line(Vec3::new(0.0, 0.07, z as f32), Vec3::new(w, 0.07, z as f32), col);
        }
    }

    // Flyers ignore the maze: show their straight route when an air wave is next or active.
    if g.wave_info(g.wave).air {
        let col = Color::srgba(0.6, 0.9, 1.0, 0.75);
        for pair in g.map.waypoints.windows(2) {
            let (a, b) = (at(pair[0], 1.5), at(pair[1], 1.5));
            let len = a.distance(b);
            let n = (len / 0.6) as i32;
            let off = (t * 1.5).fract();
            for k in 0..n {
                let t0 = ((k as f32 + off) / n as f32).min(1.0);
                let t1 = ((k as f32 + off + 0.5) / n as f32).min(1.0);
                gizmos.line(a.lerp(b, t0), a.lerp(b, t1), col);
            }
        }
    }

    // Recipe ingredients that can be forged now get a pink ring.
    for (_, ings) in g.available_recipes() {
        for i in ings {
            gizmos.circle(Isometry3d::new(at(g.cell_of(i), 0.09), up_rot), 0.52, Color::srgba(1.0, 0.45, 0.95, 0.6 + 0.4 * pulse));
        }
    }

    // A cell marked by the first of two placement clicks: a pulsing gold ghost.
    if let Some(c) = st.marked {
        let y = at(c, 0.1).y;
        let (x, z) = (c.0 as f32, c.1 as f32);
        let gold = Color::srgba(1.0, 0.82, 0.35, 0.65 + 0.35 * pulse);
        let h = 0.25 + 0.2 * pulse;
        let corners = [(x + 0.1, z + 0.1), (x + 0.9, z + 0.1), (x + 0.9, z + 0.9), (x + 0.1, z + 0.9)];
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            gizmos.line(Vec3::new(a.0, y, a.1), Vec3::new(b.0, y, b.1), gold);
            gizmos.line(Vec3::new(a.0, y, a.1), Vec3::new(x + 0.5, y + h + 0.35, z + 0.5), gold.with_alpha(0.5));
        }
    }

    // Hovered cell and path preview.
    if let Some(c) = st.hover {
        let y = at(c, 0.1).y;
        let (x, z) = (c.0 as f32, c.1 as f32);
        let mut col = Color::srgba(1.0, 0.95, 0.7, 0.9);
        if g.phase == Phase::Build && matches!(g.grid[g.idx(c)], Occ::Empty) {
            match &st.preview {
                Some((pc, rev, Some(_))) if *pc == c && *rev == g.rev => {}
                _ => col = Color::srgba(1.0, 0.25, 0.2, 1.0),
            }
        }
        let square = |gizmos: &mut Gizmos, inset: f32, h: f32, col: Color| {
            let (a, b) = (inset, 1.0 - inset);
            let pts = [Vec3::new(x + a, y + h, z + a), Vec3::new(x + b, y + h, z + a), Vec3::new(x + b, y + h, z + b), Vec3::new(x + a, y + h, z + b)];
            for k in 0..4 {
                gizmos.line(pts[k], pts[(k + 1) % 4], col);
            }
        };
        square(gizmos, 0.0, 0.0, col);
        // The controller cursor is bolder: a doubled outline and corner posts, so it's easy to
        // find on a small screen.
        if crate::pad::active() {
            square(gizmos, -0.06, 0.0, col);
            square(gizmos, 0.06, 0.0, col.with_alpha(0.6));
            let post = 0.35 + 0.15 * pulse;
            for (cx, cz) in [(x, z), (x + 1.0, z), (x + 1.0, z + 1.0), (x, z + 1.0)] {
                gizmos.line(Vec3::new(cx, y, cz), Vec3::new(cx, y + post, cz), col);
            }
            square(gizmos, 0.0, post, col.with_alpha(0.35));
        }
        if let Some(tw) = g.tower(g.idx(c)) {
            gizmos.circle(Isometry3d::new(at(c, 0.12), up_rot), g.stats(tw).range, Color::srgba(1.0, 1.0, 1.0, 0.3));
        }
    }

    // Selection: corner brackets plus range (and aura) circles.
    if let Some(i) = g.selected {
        let c = g.cell_of(i);
        let y = at(c, 0.1).y;
        let (x, z) = (c.0 as f32, c.1 as f32);
        let gold = Color::srgb(1.0, 0.85, 0.4);
        let k = 0.3;
        for (cx, cz, dx, dz) in [(x, z, 1.0, 1.0), (x + 1.0, z, -1.0, 1.0), (x, z + 1.0, 1.0, -1.0), (x + 1.0, z + 1.0, -1.0, -1.0)] {
            let p = Vec3::new(cx, y, cz);
            gizmos.line(p, p + Vec3::new(dx * k, 0.0, 0.0), gold);
            gizmos.line(p, p + Vec3::new(0.0, 0.0, dz * k), gold);
        }
        if let Some(tw) = g.tower(i) {
            let s = g.stats(tw);
            gizmos.circle(Isometry3d::new(at(c, 0.12), up_rot), s.range, Color::srgba(1.0, 0.9, 0.5, 0.6));
            if s.aura > 0.0 {
                gizmos.circle(Isometry3d::new(at(c, 0.12), up_rot), facet_core::game::AURA_RANGE, Color::srgba(1.0, 0.5, 1.0, 0.6));
            }
        }
    }
}

#[derive(Component)]
pub struct PathDot;

/// The live path: glowing dots flowing along the route the enemies will take, shown while
/// placing and choosing gems. While hovering a free cell it previews the route with a gem there.
pub fn path_dots(
    mut commands: Commands,
    mut sim: ResMut<Sim>,
    st: Res<crate::ui::UiState>,
    mut gfx: ResMut<Gfx>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut dots: Query<(&mut Transform, &mut Visibility), With<PathDot>>,
) {
    let sim = &mut *sim;
    let g = &sim.game;
    let show = sim.screen == crate::Screen::Playing && matches!(g.phase, Phase::Build | Phase::Choose);
    let (route, extra): (&[(i32, i32)], Option<(i32, i32)>) = if !show {
        (&[], None)
    } else {
        match &st.preview {
            Some((c, rev, Some(r))) if *rev == g.rev && g.phase == Phase::Build => (r, Some(*c)),
            _ => (&g.route, None),
        }
    };
    let key = (g.rev, extra);
    let pts: Vec<Vec2> = if route.is_empty() {
        Vec::new()
    } else {
        match &sim.scene.path_cache {
            Some((k, p)) if *k == key => p.clone(),
            _ => {
                // Smooth like the road, but never across towers or rocks (or the previewed gem).
                let solid = |x: i32, y: i32| {
                    let c = (x, y);
                    Some(c) == extra || (g.in_bounds(c) && !matches!(g.grid[g.idx(c)], Occ::Empty))
                };
                let p = crate::terrain::smooth_path(&g.map, route, &solid);
                sim.scene.path_cache = Some((key, p.clone()));
                p
            }
        }
    };
    let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    const SPACING: f32 = 0.5;
    let n = if pts.len() < 2 { 0 } else { (total / SPACING) as usize };
    while sim.scene.path_dots.len() < n {
        let m = gfx.mat(&mut mats, CColor::new(1.0, 0.82, 0.35, 1.0), 2, 12.0);
        let e = commands
            .spawn((Mesh3d(gfx.sphere.clone()), MeshMaterial3d(m), Transform::from_scale(Vec3::ZERO), Visibility::Hidden, NotShadowCaster, PathDot, Board))
            .id();
        sim.scene.path_dots.push(e);
    }
    let t = sim.time;
    let offset = (t * 1.4) % SPACING;
    let mut seg = 0;
    let mut seg_start = 0.0;
    for (k, &e) in sim.scene.path_dots.iter().enumerate() {
        let Ok((mut tf, mut vis)) = dots.get_mut(e) else { continue };
        if k >= n {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        let d = k as f32 * SPACING + offset;
        while seg + 2 < pts.len() && seg_start + pts[seg].distance(pts[seg + 1]) < d {
            seg_start += pts[seg].distance(pts[seg + 1]);
            seg += 1;
        }
        let (a, b) = (pts[seg], pts[seg + 1]);
        let len = a.distance(b).max(1e-4);
        let p = a.lerp(b, ((d - seg_start) / len).clamp(0.0, 1.0));
        // A pulse travels along the path towards the exit.
        let pulse = 0.5 + 0.5 * ((d - t * 6.0) * 0.6).sin();
        let y = sim.scene.ground(p.x, p.y) + 0.16;
        *tf = Transform::from_xyz(p.x, y, p.y).with_scale(Vec3::splat(0.065 + 0.04 * pulse));
        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}
