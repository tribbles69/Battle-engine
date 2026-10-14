//! Fylking: an open-source, moddable battle game in the spirit of Total War.
//!
//! This crate is just the window, rendering and controls. The battle itself
//! lives in `fylking_sim`, and the units and scenarios live in `assets/data`.

mod autoshot;
mod camera;
mod input;
mod sim;
mod ui;
mod view;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Fylking".into(),
                resolution: (1600, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(autoshot::AutoshotPlugin)
        .add_plugins((sim::SimPlugin, camera::CameraPlugin, view::ViewPlugin, input::InputPlugin, ui::UiPlugin))
        .run();
}
