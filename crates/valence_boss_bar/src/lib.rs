#![doc = include_str!("../README.md")]

use std::borrow::Cow;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use valence_server::client::{
    Client, OldViewDistance, OldVisibleEntityLayers, ViewDistance, VisibleEntityLayers,
};
use valence_server::layer::UpdateLayersPreClientSet;
pub use valence_server::protocol::packets::play::boss_event_s2c::{
    BossBarAction, BossBarColor, BossBarDivision, BossBarFlags,
};
use valence_server::protocol::packets::play::BossEventS2c;
use valence_server::protocol::WritePacket;
use valence_server::{ChunkView, Despawned, EntityLayer, Layer, UniqueId};

mod components;
pub use components::*;
use valence_entity::{EntityLayerId, OldPosition, Position};

pub struct BossBarPlugin;

impl Plugin for BossBarPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_systems(
            PostUpdate,
            (
                update_boss_bar::<BossBarTitle>,
                update_boss_bar::<BossBarHealth>,
                update_boss_bar::<BossBarStyle>,
                update_boss_bar::<BossBarFlags>,
                update_boss_bar_layer_view,
                update_boss_bar_chunk_view,
                boss_bar_despawn,
            )
                .before(UpdateLayersPreClientSet),
        );
    }
}

fn update_boss_bar<T: Component + ToPacketAction>(
    boss_bars_query: Query<(&UniqueId, &T, &EntityLayerId, Option<&Position>), Changed<T>>,
    mut entity_layers_query: Query<&mut EntityLayer>,
) {
    for (id, part, entity_layer_id, pos) in boss_bars_query.iter() {
        if let Ok(mut entity_layer) = entity_layers_query.get_mut(entity_layer_id.0) {
            let packet = BossEventS2c {
                id: id.0,
                action: part.to_packet_action(),
            };
            if let Some(pos) = pos {
                entity_layer.view_writer(pos.0).write_packet(&packet);
            } else {
                entity_layer.write_packet(&packet);
            }
        }
    }
}

fn update_boss_bar_layer_view(
    mut clients_query: Query<
        (
            &mut Client,
            &VisibleEntityLayers,
            &OldVisibleEntityLayers,
            &Position,
            &OldPosition,
            &ViewDistance,
            &OldViewDistance,
        ),
        Changed<VisibleEntityLayers>,
    >,
    boss_bars_query: Query<(
        &UniqueId,
        &BossBarTitle,
        &BossBarHealth,
        &BossBarStyle,
        &BossBarFlags,
        &EntityLayerId,
        Option<&Position>,
    )>,
) {
    for (
        mut client,
        visible_entity_layers,
        old_visible_entity_layers,
        position,
        _old_position,
        view_distance,
        _old_view_distance,
    ) in &mut clients_query
    {
        let view = ChunkView::new(position.0.into(), view_distance.get());

        let old_layers = old_visible_entity_layers.get();
        let current_layers = &visible_entity_layers.0;

        for &added_layer in current_layers.difference(old_layers) {
            for (id, title, health, style, flags, _, boss_bar_position) in boss_bars_query
                .iter()
                .filter(|(_, _, _, _, _, layer_id, _)| layer_id.0 == added_layer)
            {
                if let Some(position) = boss_bar_position {
                    if view.contains(position.0.into()) {
                        client.write_packet(&BossEventS2c {
                            id: id.0,
                            action: BossBarAction::Add {
                                title: Cow::Borrowed(&title.0),
                                health: health.0,
                                color: style.color,
                                division: style.division,
                                flags: *flags,
                            },
                        });
                    }
                } else {
                    client.write_packet(&BossEventS2c {
                        id: id.0,
                        action: BossBarAction::Add {
                            title: Cow::Borrowed(&title.0),
                            health: health.0,
                            color: style.color,
                            division: style.division,
                            flags: *flags,
                        },
                    });
                }
            }
        }

        for &removed_layer in old_layers.difference(current_layers) {
            for (id, _, _, _, _, _, boss_bar_position) in boss_bars_query
                .iter()
                .filter(|(_, _, _, _, _, layer_id, _)| layer_id.0 == removed_layer)
            {
                if let Some(position) = boss_bar_position {
                    if view.contains(position.0.into()) {
                        client.write_packet(&BossEventS2c {
                            id: id.0,
                            action: BossBarAction::Remove,
                        });
                    }
                } else {
                    client.write_packet(&BossEventS2c {
                        id: id.0,
                        action: BossBarAction::Remove,
                    });
                }
            }
        }
    }
}

fn update_boss_bar_chunk_view(
    mut clients_query: Query<
        (
            &mut Client,
            &VisibleEntityLayers,
            &OldVisibleEntityLayers,
            &Position,
            &OldPosition,
            &ViewDistance,
            &OldViewDistance,
        ),
        Changed<Position>,
    >,
    boss_bars_query: Query<(
        &UniqueId,
        &BossBarTitle,
        &BossBarHealth,
        &BossBarStyle,
        &BossBarFlags,
        &EntityLayerId,
        &Position,
    )>,
) {
    for (
        mut client,
        visible_entity_layers,
        _old_visible_entity_layers,
        position,
        old_position,
        view_distance,
        old_view_distance,
    ) in &mut clients_query
    {
        let view = ChunkView::new(position.0.into(), view_distance.get());
        let old_view = ChunkView::new(old_position.get().into(), old_view_distance.get());

        for layer in &visible_entity_layers.0 {
            for (id, title, health, style, flags, _, boss_bar_position) in boss_bars_query
                .iter()
                .filter(|(_, _, _, _, _, layer_id, _)| layer_id.0 == *layer)
            {
                if view.contains(boss_bar_position.0.into())
                    && !old_view.contains(boss_bar_position.0.into())
                {
                    client.write_packet(&BossEventS2c {
                        id: id.0,
                        action: BossBarAction::Add {
                            title: Cow::Borrowed(&title.0),
                            health: health.0,
                            color: style.color,
                            division: style.division,
                            flags: *flags,
                        },
                    });
                } else if !view.contains(boss_bar_position.0.into())
                    && old_view.contains(boss_bar_position.0.into())
                {
                    client.write_packet(&BossEventS2c {
                        id: id.0,
                        action: BossBarAction::Remove,
                    });
                }
            }
        }
    }
}

fn boss_bar_despawn(
    boss_bars_query: Query<(&UniqueId, &EntityLayerId, Option<&Position>), With<Despawned>>,
    mut entity_layer_query: Query<&mut EntityLayer>,
) {
    for (id, entity_layer_id, position) in boss_bars_query.iter() {
        if let Ok(mut entity_layer) = entity_layer_query.get_mut(entity_layer_id.0) {
            let packet = BossEventS2c {
                id: id.0,
                action: BossBarAction::Remove,
            };
            if let Some(pos) = position {
                entity_layer.view_writer(pos.0).write_packet(&packet);
            } else {
                entity_layer.write_packet(&packet);
            }
        }
    }
}
