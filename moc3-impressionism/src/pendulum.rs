use glam::Vec2;

use crate::data::PhysicsVertex;

#[derive(Clone, Copy, Debug)]
pub struct PendulumPoint {
    pub last_position: Vec2,
    pub cur_position: Vec2,
    pub cur_velocity: Vec2,
}

pub struct UpdateData {
    pub translation: Vec2,
    pub rotation: f32, // radians
}

pub struct Pendulum {
    last_global_rotation: f32,
    pub points: Vec<PendulumPoint>,
    vertexes: Vec<PhysicsVertex>,
}

impl Pendulum {
    pub fn new(vertexes: impl IntoIterator<Item = PhysicsVertex>) -> Self {
        let vertexes = vertexes.into_iter();

        let mut ret = Pendulum {
            last_global_rotation: 0.0,
            points: Vec::with_capacity(vertexes.size_hint().0),
            vertexes: Vec::with_capacity(vertexes.size_hint().0),
        };

        for vertex in vertexes {
            ret.points.push(PendulumPoint {
                last_position: vertex.position,
                cur_position: vertex.position,
                cur_velocity: Vec2::ZERO,
            });
            ret.vertexes.push(vertex);
        }

        ret
    }

    // I'm (as with most stuff here) completely unsure how Live2D actually
    // implements this, so we're left to fend on our own. This does not
    // look correct (like at all), but it's the best we got.
    //
    // This is painful. May the world fall into darkness and be reborn.
    // May life be more than a microcosm of chaos. May those who hold
    // dominion over the multiverse have mercy on my suffering.
    //
    // The following lists important things determined through watching too many examples.
    //
    // 1. Rotating everything rotates the pendulum by a factor of 1 / 5th. This feels rather
    // small but does in fact appear to hold.
    // 2. The pendulum is not a traditional (double, triple, N-pendulum) as those pendulums
    // are **far** too chaotic for this use case.
    // 3. Conservation of energy seems off - either the implementation has a bug or friction
    // is set really high by default, as the pendulums settle remarkably quickly.
    // 4. Related to the last point, I think everything is based off changes in position,
    // it doesn't feel like the movement off stuff is modeled my impulse forces,
    // and dragging things slowly versus quickly exhibits weird behavior.
    // KalEl (https://math.stackexchange.com/users/1310/kalel), https://math.stackexchange.com/q/3116
    // 5. Positive Y points down for some reason. The effective force (gravity) field
    // says gravity points down, but this does not seem to match reality.
    //
    // So, what does this mean for us? Well, the Physics SE answer below seems the closest
    // to how it actually is implemented.
    //
    // Mark H (https://physics.stackexchange.com/users/45164/mark-h), Creating a pendulum simulation in C#,
    // URL (version: 2021-06-06): https://physics.stackexchange.com/q/643629
    //
    // We ignore the conservation of energy parts, as the energy in the system decays due to air resistance
    // and the user applying energy via parameters. The settings for each bob (vertex) were determined
    // experimentally. Acceleration and radius seem pretty obvious, delay seems to have a time-slowing effect
    // and mobility is just some fudge factor applied to the velocity (maybe?, could also be accel).
    pub fn update_points(&mut self, delta_seconds: f32, update_data: UpdateData) {
        let delta_seconds = delta_seconds * 20.0;
        if delta_seconds == 0.0 {
            return;
        }

        // Rotating the entire world gives the pendulum an angle change of factor of 0.2, weird.
        let effective_rotation_change = (self.last_global_rotation - update_data.rotation) / 5.0;

        // Calculate which way gravity points, remember +y is down.
        let gravity_vector = Vec2::from(update_data.rotation.sin_cos());

        // This is technically unused, but it's kept updated for debugging reasons.
        self.points[0].last_position = self.points[0].cur_position;
        // Update the root node to the new translation
        self.points[0].cur_position = update_data.translation;
        let mut last_point = self.points[0];

        for (point, vertex) in self.points.iter_mut().zip(self.vertexes.iter()).skip(1) {
            // Last loop's current position is now this loop's last position
            point.last_position = point.cur_position;

            // The force applied to the pendulum due to gravity
            // (we assume mass is 1 for simplicity).
            let force = gravity_vector * vertex.acceleration;
            // Delay scales the passage of time - fancy time dilation!
            let effective_time = delta_seconds * vertex.delay;

            // Calculate the impact of rotating the world on the pendulum's position
            let direction = point.cur_position - last_point.cur_position;
            let rotated_dir = Vec2::from_angle(effective_rotation_change).rotate(direction);

            // Apply the contributions of the velocity and the gravity force to find the new position
            // We multiply velocity by time and force by times squared - I seem to recall a YouTube video
            // saying this is technically wrong with variable timestamps but that's a problem for future me.
            let normalized_dir = (rotated_dir
                + point.cur_velocity * effective_time
                + force * effective_time * effective_time)
                .normalize();

            // Reapply the normalized direction scaled by the radius,
            // so the pendulum bob doesn't fly off the rope.
            point.cur_position = last_point.cur_position + normalized_dir * vertex.radius;

            // I think we just calculate velocity based on how far the bob moved
            // in the given "dilated" time.
            point.cur_velocity = if effective_time == 0.0 {
                // We checked that the delta-T wasn't zero early,
                // so this effectively checks that the vertex's delay
                // is zero. (It also guards against random NaNs)
                Vec2::ZERO
            } else {
                (point.cur_position - point.last_position) / effective_time * vertex.mobility
            };
            last_point = *point;
        }

        self.last_global_rotation = update_data.rotation;
    }
}
