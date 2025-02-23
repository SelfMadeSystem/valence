use crate::layer::chunk::UnloadedChunk;
use crate::protocol::packets::play::{AddEntityS2c, PlayerInfoUpdateS2c};
use crate::testing::{create_mock_client, ScenarioSingleClient};
use crate::ChunkLayer;

#[test]
fn player_list_arrives_before_player_spawn() {
    let ScenarioSingleClient {
        mut app,
        helper: mut client_helper_1,
        layer: layer_ent,
        ..
    } = ScenarioSingleClient::new();

    let mut layer = app.world_mut().get_mut::<ChunkLayer>(layer_ent).unwrap();

    for z in -5..5 {
        for x in -5..5 {
            layer.insert_chunk([x, z], UnloadedChunk::new());
        }
    }

    app.update();

    {
        let recvd = client_helper_1.collect_received();
        recvd.assert_count::<PlayerInfoUpdateS2c>(1);
        recvd.assert_count::<AddEntityS2c>(0);
        recvd.assert_order::<(PlayerInfoUpdateS2c, AddEntityS2c)>();

        let pkt = recvd.first::<PlayerInfoUpdateS2c>();
        assert!(pkt.actions.add_player());
        assert_eq!(pkt.entries.len(), 1)
    };

    let (mut client_2, mut client_helper_2) = create_mock_client("test_2");
    client_2.player.layer.0 = layer_ent;
    client_2.visible_chunk_layer.0 = layer_ent;
    client_2.visible_entity_layers.0.insert(layer_ent);

    app.world_mut().spawn(client_2);

    app.update();

    {
        let recvd = client_helper_1.collect_received();
        recvd.assert_count::<PlayerInfoUpdateS2c>(1);
        recvd.assert_count::<AddEntityS2c>(1);
        recvd.assert_order::<(PlayerInfoUpdateS2c, AddEntityS2c)>();

        let pkt = recvd.first::<PlayerInfoUpdateS2c>();
        assert!(pkt.actions.add_player());
        assert_eq!(pkt.entries.len(), 1)
    };

    {
        let recvd = client_helper_2.collect_received();
        recvd.assert_count::<PlayerInfoUpdateS2c>(1);
        recvd.assert_count::<AddEntityS2c>(1);
        recvd.assert_order::<(PlayerInfoUpdateS2c, AddEntityS2c)>();

        let pkt = recvd.first::<PlayerInfoUpdateS2c>();
        assert!(pkt.actions.add_player());
        assert_eq!(pkt.entries.len(), 2)
    };
}
