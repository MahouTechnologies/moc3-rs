use std::cmp::Ordering;

use indextree::{Arena, NodeId};

use super::PuppetFrameData;

#[derive(Debug, Clone)]
pub enum DrawOrderNode {
    ArtMesh { index: u32 },
    Part { index: u32 },
}

// This is such a hack. Processing this draw order tree requires so much allocating and pointer indirection and just mucking
// around with data. Sigh. I think I just bite the bullet and use recursion here or something, I'm really not sure
// how this is supposed to be done otherwise.
//
// I feel like I'm missing something simple but it doesn't appear like I am alas. I want to boil this down into a simple array
// I can sort, but I feel like that loses precision somehow - a f32 only has 32 bits of precision, and each draw order group contains
// 8 bits (0-1000), so I can nest at most 4 in a row with that implmentation.
fn draw_order_tree_rec(
    draw_order_nodes: &Arena<DrawOrderNode>,
    draw_order_root: NodeId,
    cur_index: &mut usize,
    frame_data: &mut PuppetFrameData,
) {
    let mut orders: Vec<(f32, NodeId)> = Vec::new();
    for i in draw_order_root.children(&draw_order_nodes) {
        let data = draw_order_nodes[i].get();

        match data {
            DrawOrderNode::ArtMesh { index } => {
                orders.push((frame_data.art_mesh_draw_orders[*index as usize].round(), i));
            }
            DrawOrderNode::Part { .. } => {
                // I haven't done parts yet
                orders.push((500.0, i));
            }
        }
    }
    orders.sort_unstable_by(|a, b| {
        let first = a.0.total_cmp(&b.0);
        if first == Ordering::Equal {
            a.1.cmp(&b.1)
        } else {
            first
        }
    });

    for (_, id) in orders {
        let child = draw_order_nodes[id].get();

        match child {
            DrawOrderNode::ArtMesh { index: part_index } => {
                frame_data.art_mesh_render_orders[*cur_index] = *part_index;
                *cur_index += 1;
            }
            DrawOrderNode::Part { .. } => {
                draw_order_tree_rec(draw_order_nodes, id, cur_index, frame_data);
            }
        }
    }
}

pub fn draw_order_tree(
    draw_order_nodes: &Arena<DrawOrderNode>,
    draw_order_root: NodeId,
    frame_data: &mut PuppetFrameData,
) {
    draw_order_tree_rec(draw_order_nodes, draw_order_root, &mut 0, frame_data);
}
