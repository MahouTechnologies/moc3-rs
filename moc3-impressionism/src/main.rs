use data::physics::{Physics3Data, PhysicsVertex};
use glam::Vec2;

mod data;
mod pendulum;

pub fn main() {
    let string = std::fs::read_to_string("test.physics3.json").unwrap();
    let deserialized: Physics3Data = serde_json::from_str(&string).unwrap();

    println!("{:#?}", deserialized);
}

// mobility = shaking influence
// delay = reaction time
// acceleration = overall acceleration
// radius = duration
