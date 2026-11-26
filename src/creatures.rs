//! Animated creature models (CC0 "Cute Animated Monsters" by Quaternius, converted to glTF).
//! `assets/models/manifest.json` records each model's bounds and which animation is which.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use serde::Deserialize;

#[derive(Deserialize)]
struct Entry {
    min: [f32; 3],
    max: [f32; 3],
    walk: Option<usize>,
    fly: Option<usize>,
    death: Option<usize>,
}

pub struct Model {
    scene: Handle<WorldAsset>,
    graph: Handle<AnimationGraph>,
    walk: Option<AnimationNodeIndex>,
    fly: Option<AnimationNodeIndex>,
    death: Option<AnimationNodeIndex>,
    /// Model units to world units for a creature one unit tall.
    unit: f32,
}

#[derive(Resource, Default)]
pub struct Models(HashMap<String, Model>, std::collections::HashSet<Entity>);

/// Which model each creature look uses.
fn model_for(look: &str) -> &'static str {
    match look {
        "beetle" => "Cyclops",
        "runner" => "Deer",
        "brute" => "GreenDemon",
        "armored" => "Crab",
        "swarm" => "Bee",
        "bat" => "Bat",
        "golem" => "Yeti",
        "wraith" => "Ghost",
        "hydra" => "YellowDragon",
        "hydraling" => "Alien",
        "leviathan" => "Cthulhu",
        "shade" => "Skull",
        "mender" => "Tree",
        "guardian" => "Panda",
        "imp" => "Demon",
        "brood" => "Alien_Tall",
        "warded" => "Cactus",
        _ => "Cyclops",
    }
}

impl Models {
    pub fn load(asset_dir: &str, server: &AssetServer, graphs: &mut Assets<AnimationGraph>) -> Models {
        let path = std::path::Path::new(asset_dir).join("models/manifest.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("creature models not found at {} (using placeholders)", path.display());
            return Models::default();
        };
        let Ok(entries) = serde_json::from_str::<HashMap<String, Entry>>(&text) else {
            eprintln!("could not read {}", path.display());
            return Models::default();
        };
        let mut out = HashMap::new();
        for (name, e) in entries {
            let file = format!("models/{name}.gltf");
            let clip = |i: Option<usize>| i.map(|i| server.load(GltfAssetLabel::Animation(i).from_asset(file.clone())));
            let clips: Vec<Option<Handle<AnimationClip>>> = vec![clip(e.walk), clip(e.fly), clip(e.death)];
            let present: Vec<Handle<AnimationClip>> = clips.iter().flatten().cloned().collect();
            let (graph, nodes) = AnimationGraph::from_clips(present);
            let mut it = nodes.into_iter();
            let mut next = |c: &Option<Handle<AnimationClip>>| c.as_ref().and_then(|_| it.next());
            let walk = next(&clips[0]);
            let fly = next(&clips[1]);
            let death = next(&clips[2]);
            let height = (e.max[1] - e.min[1]).max(1e-3);
            out.insert(
                name,
                Model {
                    scene: server.load(GltfAssetLabel::Scene(0).from_asset(file.clone())),
                    graph: graphs.add(graph),
                    walk,
                    fly,
                    death,
                    unit: 1.0 / height,
                },
            );
        }
        Models(out, Default::default())
    }

    pub fn has(&self, look: &str) -> bool {
        self.0.contains_key(model_for(look))
    }

    /// Remembers that a creature root uses a model (so it can play a death animation).
    pub fn mark(&mut self, e: Entity) {
        self.1.insert(e);
    }

    pub fn has_entity(&mut self, e: Entity) -> bool {
        self.1.remove(&e)
    }
}

/// Drives a creature's animations. Lives on the model entity (child of the creature root).
#[derive(Component)]
pub struct Rig {
    graph: Handle<AnimationGraph>,
    moving: Option<AnimationNodeIndex>,
    death: Option<AnimationNodeIndex>,
    speed: f32,
    player: Option<Entity>,
}

/// A killed creature playing its death animation before it disappears.
#[derive(Component)]
pub struct Corpse {
    t: f32,
}

impl Corpse {
    pub fn new() -> Corpse {
        Corpse { t: 0.0 }
    }
}

/// Spawns the model under `parent`. `height` is the creature's height in world units.
pub fn spawn(commands: &mut Commands, models: &Models, parent: Entity, look: &str, height: f32, air: bool, anim_speed: f32) -> Option<(Entity, f32)> {
    let m = models.0.get(model_for(look))?;
    let moving = if air { m.fly.or(m.walk) } else { m.walk.or(m.fly) };
    let s = m.unit * height;
    let child = commands
        .spawn((
            WorldAssetRoot(m.scene.clone()),
            Transform::from_scale(Vec3::splat(s)),
            Rig { graph: m.graph.clone(), moving, death: m.death, speed: anim_speed, player: None },
        ))
        .observe(start_animation)
        .id();
    commands.entity(parent).add_child(child);
    Some((child, s))
}

fn start_animation(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    mut rigs: Query<&mut Rig>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok(mut rig) = rigs.get_mut(ready.entity) else { return };
    for child in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            if let Some(node) = rig.moving {
                // Start creatures at different points in the cycle so crowds don't march in step.
                let offset = (child.index().index() % 17) as f32 * 0.07;
                player.play(node).repeat().set_speed(rig.speed).seek_to(offset);
            }
            commands.entity(child).insert(AnimationGraphHandle(rig.graph.clone()));
            rig.player = Some(child);
            break;
        }
    }
}

/// Plays death animations, then sinks and removes the body.
pub fn corpses(
    mut commands: Commands,
    time: Res<Time>,
    mut bodies: Query<(Entity, &mut Corpse, &mut Transform, &Children)>,
    rigs: Query<&Rig>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let dt = time.delta_secs();
    for (e, mut corpse, mut tf, kids) in &mut bodies {
        if corpse.t == 0.0 {
            for k in kids.iter() {
                if let Ok(rig) = rigs.get(k)
                    && let Some(p) = rig.player
                    && let Ok(mut player) = players.get_mut(p)
                {
                    player.stop_all();
                    if let Some(d) = rig.death {
                        player.play(d).set_speed(1.4);
                    }
                }
            }
        }
        corpse.t += dt;
        if corpse.t > 0.9 {
            tf.translation.y -= dt * 0.8;
            tf.scale *= 1.0 - dt * 1.5;
        }
        if corpse.t > 1.6 {
            commands.entity(e).despawn();
        }
    }
}
