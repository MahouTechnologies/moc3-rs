use glam::Vec2;

#[derive(Clone, Debug)]
pub struct PendulumPoint {
    /// The position on the current timestep.
    position: Vec2,
    /// The gravity vector on the current timestep.
    gravity: Vec2,
    /// The velocity vector on the current timestep.
    velocity: Vec2,
    /// The position on the previous timestep.
    last_position: Vec2,

    mobility: f32,
    delay: f32,
    acceleration: f32,
    radius: f32,
}

impl PendulumPoint {
    pub fn position(&self) -> Vec2 {
        self.position
    }

    pub fn velocity(&self) -> Vec2 {
        self.velocity
    }
}

/// Input state supplied to [`Pendulum::update_points`] each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateData {
    /// Translation of the pendulum root.
    pub translation: Vec2,
    /// Rotation in radians.
    pub rotation: f32,
}

// I'm (as with most stuff here) completely unsure how Live2D actually
// implements this, so we're left to fend on our own. I have gotten a bit of
// assistance from soneone with who collected a ton of data for me,
// so this should be somewhat accurate.
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
pub struct Pendulum {
    points: Vec<PendulumPoint>,
}

impl Pendulum {
    /// As mentioned above, rotating the pendulum by X actually
    /// rotates it by 1/5th for some reason.
    const ROTATION_SCALE_FACTOR: f32 = 5.0;

    /// The default gravity for the physics simulation, remember
    /// +Y is downwards.
    const DEFAULT_GRAVITY: Vec2 = Vec2::new(0.0, 1.0);

    /// 1 second of real time goes by faster in the simulation.
    const SECONDS_PER_REAL_SECOND: f32 = 25.0;

    /// Construct a pendulum from vertex descriptors.
    pub fn new(vertices: impl IntoIterator<Item = crate::data::PhysicsVertex>) -> Self {
        let points: Vec<PendulumPoint> = vertices
            .into_iter()
            .map(|v| PendulumPoint {
                position: v.position,
                last_position: v.position,
                velocity: Vec2::ZERO,
                gravity: Vec2::new(0.0, 1.0),
                mobility: v.mobility,
                delay: v.delay,
                acceleration: v.acceleration,
                radius: v.radius,
            })
            .collect();

        Self { points }
    }

    pub fn points(&self) -> &[PendulumPoint] {
        &self.points
    }

    /// Reset all points to a rest configuration along +Y, based on link radii.
    pub fn reset(&mut self) {
        if self.points.is_empty() {
            return;
        }

        // We check emptiness above
        let first = self.points.first_mut().unwrap();
        first.position = Vec2::ZERO;
        first.last_position = Vec2::ZERO;
        first.gravity = Self::DEFAULT_GRAVITY;
        first.velocity = Vec2::ZERO;

        // I wish we had lending iterators.
        for i in 1..self.points.len() {
            let radius = self.points[i].radius;
            let prev_init = self.points[i - 1].position;
            let init_pos = prev_init + Vec2::new(0.0, radius);
            self.points[i].position = init_pos;
            self.points[i].last_position = init_pos;
            self.points[i].gravity = Self::DEFAULT_GRAVITY;
            self.points[i].velocity = Vec2::ZERO;
        }
    }

    /// Advance by `delta_seconds` with the given state.
    pub fn step(&mut self, delta_seconds: f32, data: UpdateData, wind: Vec2, threshold: f32) {
        if delta_seconds <= 0.0 {
            return;
        }

        if self.points.is_empty() {
            return;
        }

        self.points[0].last_position = self.points[0].position;
        self.points[0].position = data.translation;

        let current_gravity = Vec2::from(data.rotation.sin_cos()).normalize_or_zero();

        // I wish we had lending iterators.
        for i in 1..self.points.len() {
            // The force applied to the pendulum due to gravity
            // and wind (we assume mass is 1 for simplicity).
            let net_force = current_gravity * self.points[i].acceleration + wind;
            self.points[i].last_position = self.points[i].position;

            // Delay scales the passage of time - fancy time dilation!
            let effective_time =
                self.points[i].delay * delta_seconds * Self::SECONDS_PER_REAL_SECOND;

            // Calculate the impact of rotating the world on the pendulum's position
            let direction = self.points[i].position - self.points[i - 1].position;
            // Rotate the current segment by the gravity change, scaled by rotation factor.
            let radian =
                self.points[i].gravity.angle_to(current_gravity) / Self::ROTATION_SCALE_FACTOR;

            // We can use from_angle here because everything is self consistent I'm pretty sure.
            let rotated_direction = direction.rotate(Vec2::from_angle(radian));

            // Apply velocity and force contributions.
            let normalized_dir = (rotated_direction
                + self.points[i].velocity * effective_time
                + net_force * effective_time * effective_time)
                .normalize_or_zero();

            // Reapply the normalized direction scaled by the radius,
            // so the pendulum bob doesn't fly off the rope.
            self.points[i].position =
                self.points[i - 1].position + normalized_dir * self.points[i].radius;

            if self.points[i].position.x.abs() < threshold {
                self.points[i].position.x = 0.0;
            }

            if effective_time != 0.0 {
                self.points[i].velocity = (self.points[i].position - self.points[i].last_position)
                    / effective_time
                    * self.points[i].mobility;
            }

            self.points[i].gravity = current_gravity;
        }
    }

    /// Find fixpoint with the given state.
    pub fn fixpoint(&mut self, data: UpdateData, wind: Vec2, threshold: f32) {
        if self.points.is_empty() {
            return;
        }

        self.points[0].last_position = self.points[0].position;
        self.points[0].position = data.translation;

        let current_gravity = Vec2::from(data.rotation.sin_cos()).normalize_or_zero();

        // I still wish we had lending iterators.
        for i in 1..self.points.len() {
            let net_force = current_gravity * self.points[i].acceleration + wind;
            self.points[i].last_position = self.points[i].position;
            self.points[i].velocity = Vec2::ZERO;

            self.points[i].position =
                self.points[i - 1].position + net_force.normalize_or_zero() * self.points[i].radius;

            // Correct for jitter when the numbers get too small.
            if self.points[i].position.x.abs() < threshold {
                self.points[i].position.x = 0.0;
            }

            self.points[i].gravity = current_gravity;
        }
    }
}
