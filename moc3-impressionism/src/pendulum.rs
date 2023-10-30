use glam::Vec2;

struct PendulumPoint {
    start_point: Vec2,
    cur_point: Vec2,
    cur_velocity: Vec2,
}

pub struct Pendulum {
    root: Vec2,
    vertexes: Vec<PendulumPoint>,
}

impl Pendulum {
    pub fn update(&mut self) {
        for vertex in &mut self.vertexes {
            // vertex.
        }
    }
}