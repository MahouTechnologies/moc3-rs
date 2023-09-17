use glam::Vec2;

// Glues are thankfully rather simple, they "glue" two
// vertexes together. The key thing is that there's an
// intensity value for how strong the glue is, and also
// weights for figuring out which side has a stronger
// pull.

pub fn apply_glue(
    intensity: f32,
    positions: &[u16],
    weights: &[f32],
    art_mesh_one: &mut [Vec2],
    art_mesh_two: &mut [Vec2],
) {
    debug_assert_eq!(positions.len(), weights.len());

    for (index, weight) in positions.chunks_exact(2).zip(weights.chunks_exact(2)) {
        let a = art_mesh_one[index[0] as usize];
        let b = art_mesh_two[index[1] as usize];

        art_mesh_one[index[0] as usize] += (b - a) * weight[0] * intensity;
        art_mesh_two[index[1] as usize] += (a - b) * weight[1] * intensity;
    }
}
