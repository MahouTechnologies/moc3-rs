use std::time::Instant;

use eframe::{
    egui::{self, Sense},
    epaint::{Color32, Pos2, Stroke, Vec2},
};
use moc3_impressionism::{Pendulum, PhysicsVertex, UpdateData};

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 800.0])
            .with_resizable(false),
        ..Default::default()
    };

    let mut physics = Pendulum::new([
        PhysicsVertex {
            position: glam::Vec2::new(0.0, 0.0),
            mobility: 1.0,
            delay: 1.0,
            acceleration: 1.0,
            radius: 0.0,
        },
        PhysicsVertex {
            position: glam::Vec2::new(0.0, 3.0),
            mobility: 0.95,
            delay: 0.8,
            acceleration: 1.5,
            radius: 3.0,
        },
        PhysicsVertex {
            position: glam::Vec2::new(0.0, 6.0),
            mobility: 0.95,
            delay: 0.8,
            acceleration: 1.5,
            radius: 3.0,
        },
        PhysicsVertex {
            position: glam::Vec2::new(0.0, 9.0),
            mobility: 0.95,
            delay: 0.8,
            acceleration: 1.5,
            radius: 3.0,
        },
    ]);

    let mut last = None;
    let mut translation = glam::Vec2::ZERO;
    let mut rotation = 0.0;
    eframe::run_ui_native("My egui App", options, move |ctx, _frame| {
        egui::CentralPanel::default().show_inside(ctx, |ui| {
            if ui.input(|i| i.key_pressed(egui::Key::A)) {
                translation.x += 2.0;
            }
            if ui.input(|i| i.key_pressed(egui::Key::D)) {
                translation.x -= 2.0;
            }

            match ui.input(|i| (i.key_pressed(egui::Key::Q), i.modifiers.shift)) {
                (true, true) => rotation += std::f32::consts::FRAC_PI_4,
                (true, false) => rotation += std::f32::consts::FRAC_PI_8,
                _ => {}
            }

            match ui.input(|i| (i.key_pressed(egui::Key::E), i.modifiers.shift)) {
                (true, true) => rotation -= std::f32::consts::FRAC_PI_4,
                (true, false) => rotation -= std::f32::consts::FRAC_PI_8,
                _ => {}
            }

            if last.is_none() {
                last = Some(Instant::now());
            }
            let now = Instant::now();
            physics.step(
                (now - last.unwrap()).as_secs_f32(),
                UpdateData {
                    translation,
                    rotation,
                },
                Default::default(),
                0.001,
            );
            last = Some(now);

            let origin = Pos2::new(400.0, 400.0);
            let (_response, painter) = ui.allocate_painter(Vec2::splat(800.0), Sense::hover());

            let points = physics.points();
            let mut last_point = Vec2::from(points[0].position().to_array());
            let scale_factor = 20.0;
            painter.circle(
                origin + last_point * scale_factor,
                20.0,
                Color32::TRANSPARENT,
                Stroke::new(2.0, Color32::RED),
            );
            for point in points.iter().map(|x| x.position()).skip(1) {
                let next = Vec2::from(point.to_array());

                painter.line_segment(
                    [
                        origin + last_point * scale_factor,
                        origin + next * scale_factor,
                    ],
                    Stroke::new(2.0, Color32::RED),
                );
                painter.circle(
                    origin + next * scale_factor,
                    20.0,
                    Color32::TRANSPARENT,
                    Stroke::new(2.0, Color32::RED),
                );

                last_point = next;
            }
        });
        ctx.request_repaint();
    })
}
