use bevy::{camera::primitives::Aabb, prelude::*};
use std::any::TypeId;

use glow::HasContext;

use crate::{
    BevyGlContext,
    plane_reflect::ReflectionPlane,
    render::{RenderPhase, RenderRunner, RenderSet},
};

pub struct TransparentPhasePlugin;
impl Plugin for TransparentPhasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeferredAlphaBlendDraws>();
        app.add_systems(
            PostUpdate,
            clear_alpha_blend_draws.in_set(RenderSet::Prepare),
        );
        app.add_systems(
            PostUpdate,
            (
                render_reflect_transparent.in_set(RenderSet::RenderReflectTransparent),
                render_transparent.in_set(RenderSet::RenderTransparent),
            ),
        );
    }
}

#[derive(Resource, Default)]
pub struct DeferredAlphaBlendDraws {
    pub deferred: Vec<(f32, Entity, TypeId)>,
    pub next: Vec<Entity>,
}

impl DeferredAlphaBlendDraws {
    // Defer an entity to be drawn in the alpha blend phase
    pub fn defer<T: ?Sized + 'static>(&mut self, distance: f32, entity: Entity) {
        self.deferred.push((distance, entity, TypeId::of::<T>()));
    }

    // Returns whether to draw or not depending on phase.
    // If in opaque phase we must defer any alpha blend draws so they can be sorted and run in order.
    pub fn maybe_defer<T: ?Sized + 'static>(
        &mut self,
        transparent_draw: bool,
        phase: RenderPhase,
        entity: Entity,
        transform: &GlobalTransform,
        aabb: &Aabb,
        view_from_world: &Mat4,
        world_from_local: &Mat4,
    ) -> bool {
        if !transparent_draw {
            return true;
        }
        if phase.opaque() {
            let ws_radius = transform.radius_vec3a(aabb.half_extents);
            let ws_center = world_from_local.transform_point3a(aabb.center);
            self.defer::<T>(
                // Use closest point on bounding sphere
                view_from_world.project_point3a(ws_center).z + ws_radius,
                entity,
            );
        }
        phase.transparent()
    }

    // Take the current set of alpha blend entities to be drawn
    pub fn take(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.next)
    }
}

fn clear_alpha_blend_draws(world: &mut World) {
    world
        .get_resource_mut::<DeferredAlphaBlendDraws>()
        .unwrap()
        .deferred
        .clear();
}

fn render_reflect_transparent(world: &mut World) {
    let mut planes = world.query::<&ReflectionPlane>();
    if planes.iter(world).len() == 0 {
        return;
    }
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::ReflectTransparent;
    transparent(world);
}

fn render_transparent(world: &mut World) {
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Transparent;
    transparent(world);
}

fn transparent(world: &mut World) {
    world
        .get_non_send_resource_mut::<BevyGlContext>()
        .unwrap()
        .start_alpha_blend();
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Transparent;

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };
    for system in &runner.prepare_registry {
        let _ = world.run_system(*system);
    }

    {
        let mut draws = world.get_resource_mut::<DeferredAlphaBlendDraws>().unwrap();
        draws
            .deferred
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        draws.next.clear();
    }

    let mut current_type_id = None;
    let mut last = false;
    // Draw deferred transparent
    loop {
        let mut draws = world.get_resource_mut::<DeferredAlphaBlendDraws>().unwrap();
        // collect draws off the end of draws.deferred on to draws.next until we hit a different id, then submit those
        // before collecting the next set
        loop {
            if let Some((dist, entity, type_id)) = draws.deferred.pop() {
                if let Some(last_type_id) = current_type_id {
                    if last_type_id == type_id {
                        draws.next.push(entity);
                    } else {
                        draws.deferred.push((dist, entity, type_id));
                        current_type_id = None;
                        break;
                    }
                } else {
                    draws.next.clear();
                    draws.next.push(entity);
                    current_type_id = Some(type_id);
                }
            } else {
                last = true;
                break;
            }
        }

        if let Some(current_type_id) = current_type_id {
            let _ = world.run_system(*runner.render_registry.get(&current_type_id).unwrap());
        } else {
            break;
        }
        if last {
            break;
        }
    }

    let ctx = world.non_send_resource::<BevyGlContext>();
    unsafe { ctx.gl.bind_vertex_array(None) };
    world.insert_resource(runner);

    world
        .get_resource_mut::<DeferredAlphaBlendDraws>()
        .unwrap()
        .deferred
        .clear();
}
