//! Ron file loading for event definitions.
//!
//! Loads story events and random event pools from `.ron` assets.

use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

use super::{EventDef, EventsData};

/// ron file format for story events.
#[derive(Debug, Clone, Deserialize)]
struct StoryEventsFile {
    events: Vec<EventDef>,
}

/// ron file format for a random event pool.
#[derive(Debug, Clone, Deserialize)]
struct RandomPoolFile {
    events: Vec<EventDef>,
}

fn load_ron_file<T: for<'de> Deserialize<'de>>(path: &str) -> Option<T> {
    use std::fs;
    match fs::read_to_string(path) {
        Ok(contents) => ron::from_str(&contents).ok(),
        Err(e) => {
            warn!("Failed to read {}: {}", path, e);
            None
        }
    }
}

/// Plugin that loads event definitions and registers the EventsData resource.
pub struct EventsPlugin;

impl Plugin for EventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_events_data);
    }
}

/// Load all event definitions from ron files.
pub fn load_events_data(mut commands: Commands) {
    info!("Loading event definitions...");

    let mut data = EventsData::default();

    // Load story events
    if let Some(file) = load_ron_file::<StoryEventsFile>("assets/data/events/story_act1.ron") {
        for event in file.events {
            let id = event.id.clone();
            data.story_events.insert(id, event);
        }
        info!("Loaded {} story events", data.story_events.len());
    } else {
        warn!("No story events file found");
    }

    // Load random pools
    for (pool_id, filename) in [
        ("discovery", "random_discovery.ron"),
        ("disaster", "random_disaster.ron"),
        ("opportunity", "random_opportunity.ron"),
    ] {
        let path = format!("assets/data/events/{}", filename);
        if let Some(file) = load_ron_file::<RandomPoolFile>(&path) {
            let pool = match pool_id {
                "discovery" => &mut data.discovery_pool,
                "disaster" => &mut data.disaster_pool,
                "opportunity" => &mut data.opportunity_pool,
                _ => continue,
            };
            *pool = file.events;
            info!("Loaded {} {} events", pool.len(), pool_id);
        }
    }

    commands.insert_resource(data);
}