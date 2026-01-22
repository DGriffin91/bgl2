use bevy::{
    mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    prelude::*,
};

use crate::render::RenderSet;

/// Handles updating joint matrices
pub struct PrepareJointsPlugin;

impl Plugin for PrepareJointsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (init_bindposes, update_bindposes)
                .chain()
                .in_set(RenderSet::Prepare),
        );
    }
}

#[derive(Component, Clone, Deref, DerefMut, Default)]
pub struct JointData(Vec<Mat4>);

pub fn init_bindposes(
    mut commands: Commands,
    joint_query: Query<&GlobalTransform>,
    inverse_bindposes: Res<Assets<SkinnedMeshInverseBindposes>>,
    mut mesh_entities: Query<(Entity, &SkinnedMesh), Without<JointData>>,
) {
    mesh_entities.iter_mut().for_each(|(entity, skinned_mesh)| {
        let mut joint_data = JointData::default();
        skinned_mesh_joints(
            skinned_mesh,
            &inverse_bindposes,
            &joint_query,
            &mut joint_data,
        );
        if !joint_data.is_empty() {
            commands.entity(entity).insert(joint_data);
        }
    });
}

pub fn update_bindposes(
    joint_query: Query<&GlobalTransform>,
    inverse_bindposes: Res<Assets<SkinnedMeshInverseBindposes>>,
    mut mesh_entities: Query<(&SkinnedMesh, &mut JointData)>,
) {
    mesh_entities
        .par_iter_mut()
        .for_each(|(skinned_mesh, mut joints_data)| {
            skinned_mesh_joints(
                skinned_mesh,
                &inverse_bindposes,
                &joint_query,
                &mut joints_data,
            );
        });
}

#[inline]
pub fn skinned_mesh_joints(
    skin: &SkinnedMesh,
    inverse_bindposes: &Assets<SkinnedMeshInverseBindposes>,
    joints: &Query<&GlobalTransform>,
    joint_data: &mut Vec<Mat4>,
) {
    joint_data.clear();
    let Some(inverse_bindposes) = inverse_bindposes.get(&skin.inverse_bindposes) else {
        return;
    };

    for (inverse_bindpose, joint) in inverse_bindposes.iter().zip(skin.joints.iter()) {
        if let Ok(joint) = joints.get(*joint) {
            joint_data.push(joint.affine() * *inverse_bindpose);
        } else {
            return;
        }
    }
}
