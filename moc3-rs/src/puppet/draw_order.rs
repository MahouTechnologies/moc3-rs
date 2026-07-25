use super::PuppetFrameData;

/// A draw order group: a contiguous run of [`DrawObject`]s in the object list.
#[derive(Debug, Clone)]
pub struct DrawGroup {
    pub object_start: u32,
    pub object_count: u32,
    /// Count of art meshes drawn through this group's entire subtree.
    pub total_count: u32,
    pub min_order: i32,
    pub max_order: i32,
}

#[derive(Debug, Clone)]
pub struct DrawObject {
    pub is_part: bool,
    /// Art mesh index, or part index.
    pub index: u32,
    /// For a part, the group holding what it draws; `None` if it draws nothing.
    pub owned_group: Option<u32>,
}

/// Order the art meshes for drawing.
pub fn sort_render_order(
    groups: &[DrawGroup],
    objects: &[DrawObject],
    frame_data: &mut PuppetFrameData,
) {
    let PuppetFrameData {
        part_enabled,
        art_mesh_enabled,
        part_draw_orders,
        art_mesh_draw_orders,
        art_mesh_opacities,
        draw_group_cursors,
        sorted_objects,
        render_sequence,
        art_mesh_render_orders,
        ..
    } = frame_data;

    art_mesh_render_orders.clear();
    if groups.is_empty() {
        return;
    }

    const UNPLACED: u32 = u32::MAX;

    render_sequence.fill(UNPLACED);
    draw_group_cursors.fill(0);

    for (gi, group) in groups.iter().enumerate() {
        let count = group.object_count as usize;
        if count == 0 {
            continue;
        }
        let start = group.object_start as usize;

        // Sort this group's objects back to front. Use a tiebreak so
        // we can use a non-allocating unstable sort.
        let sorted = &mut sorted_objects[..count];
        for (j, slot) in sorted.iter_mut().enumerate() {
            *slot = j as u32;
        }
        sorted.sort_unstable_by_key(|&j| {
            let object = &objects[start + j as usize];
            let i = object.index as usize;

            // Disabled objects drop to the group minimum, sinking to the front.
            let order = if object.is_part {
                part_enabled[i].then(|| part_draw_orders[i])
            } else {
                art_mesh_enabled[i].then(|| art_mesh_draw_orders[i])
            };
            let order = order.map_or(group.min_order, |o| o.round() as i32);
            (order.clamp(group.min_order, group.max_order), j)
        });

        // Place meshes down from this group's cursor.
        let mut pos = draw_group_cursors[gi] as usize;
        for &j in sorted.iter() {
            let object = &objects[start + j as usize];

            if object.is_part {
                if let Some(owned) = object.owned_group {
                    let owned = owned as usize;
                    debug_assert!(owned > gi, "draw order group {owned} is not after {gi}");
                    draw_group_cursors[owned] = pos as u32;
                    pos += groups[owned].total_count as usize;
                }
            } else {
                debug_assert!(pos < render_sequence.len());
                render_sequence[pos] = object.index;
                pos += 1;
            }
        }
    }

    for &art_mesh in render_sequence.iter() {
        if art_mesh == UNPLACED {
            continue;
        }
        let i = art_mesh as usize;
        if art_mesh_enabled[i] && art_mesh_opacities[i] != 0.0 {
            art_mesh_render_orders.push(art_mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puppet::framedata_for_tests;

    fn model() -> (Vec<DrawGroup>, Vec<DrawObject>) {
        let groups = vec![
            DrawGroup {
                object_start: 0,
                object_count: 3,
                total_count: 4,
                min_order: 0,
                max_order: 10,
            },
            DrawGroup {
                object_start: 3,
                object_count: 2,
                total_count: 2,
                min_order: 0,
                max_order: 10,
            },
        ];
        let objects = vec![
            DrawObject {
                is_part: false,
                index: 0,
                owned_group: None,
            },
            DrawObject {
                is_part: true,
                index: 0,
                owned_group: Some(1),
            },
            DrawObject {
                is_part: false,
                index: 1,
                owned_group: None,
            },
            DrawObject {
                is_part: false,
                index: 2,
                owned_group: None,
            },
            DrawObject {
                is_part: false,
                index: 3,
                owned_group: None,
            },
        ];
        (groups, objects)
    }

    #[test]
    fn part_drawn_by_given_order() {
        let (groups, objects) = model();
        let mut frame = framedata_for_tests(4, 1, &groups);

        frame.art_mesh_draw_orders = vec![1.0, 9.0, 3.0, 7.0];
        frame.part_draw_orders = vec![5.0];

        sort_render_order(&groups, &objects, &mut frame);
        assert_eq!(frame.art_mesh_render_orders, vec![0, 2, 3, 1]);

        frame.part_draw_orders = vec![0.0];
        sort_render_order(&groups, &objects, &mut frame);
        assert_eq!(frame.art_mesh_render_orders, vec![2, 3, 0, 1]);
    }

    #[test]
    fn equal_draw_orders_keep_file_order() {
        let (groups, objects) = model();
        let mut frame = framedata_for_tests(4, 1, &groups);

        frame.art_mesh_draw_orders = vec![5.0; 4];
        frame.part_draw_orders = vec![5.0];

        sort_render_order(&groups, &objects, &mut frame);
        assert_eq!(frame.art_mesh_render_orders, vec![0, 2, 3, 1]);
    }

    #[test]
    fn disabled_part_sinks_to_front() {
        let (groups, objects) = model();
        let mut frame = framedata_for_tests(4, 1, &groups);

        frame.art_mesh_draw_orders = vec![1.0, 9.0, 3.0, 7.0];
        frame.part_draw_orders = vec![5.0];
        frame.part_enabled = vec![false];

        sort_render_order(&groups, &objects, &mut frame);
        assert_eq!(frame.art_mesh_render_orders, vec![2, 3, 0, 1]);
    }

    #[test]
    fn disabled_part_takes_everything_under_it_with_it() {
        let (groups, objects) = model();
        let mut frame = framedata_for_tests(4, 1, &groups);

        frame.art_mesh_draw_orders = vec![1.0, 9.0, 3.0, 7.0];
        frame.part_draw_orders = vec![5.0];

        frame.part_enabled = vec![false];
        frame.art_mesh_enabled[2] = false;
        frame.art_mesh_enabled[3] = false;

        sort_render_order(&groups, &objects, &mut frame);
        assert_eq!(frame.art_mesh_render_orders, vec![0, 1]);
    }

    #[test]
    fn invisible_meshes_ignored() {
        let (groups, objects) = model();
        let mut frame = framedata_for_tests(4, 1, &groups);

        frame.art_mesh_draw_orders = vec![1.0, 9.0, 3.0, 7.0];
        frame.part_draw_orders = vec![5.0];

        frame.art_mesh_enabled[2] = false;
        frame.art_mesh_opacities[0] = 0.0;

        sort_render_order(&groups, &objects, &mut frame);
        assert_eq!(frame.art_mesh_render_orders, vec![3, 1]);
    }
}
