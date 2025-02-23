use std::collections::HashMap;

use heck::ToPascalCase;
use proc_macro2::TokenStream;
use quote::quote;
use serde::Deserialize;
use valence_build_utils::write_generated_file;

#[derive(Deserialize)]
struct Packet {
    name: String,
    side: String,
    phase: String,
    id: i32,
}

pub fn main() -> anyhow::Result<()> {
    let packets: Vec<Packet> = serde_json::from_str(include_str!("extracted/packets.json"))?;

    write_packets(&packets)?;
    write_transformer(&packets)?;

    Ok(())
}

fn write_packets(packets: &Vec<Packet>) -> anyhow::Result<()> {
    let mut consts = quote! {
        #[allow(clippy::unseparated_literal_suffix)]

    };

    let len = packets.len();

    let mut p: Vec<TokenStream> = Vec::new();

    for packet in packets {
        let name = packet.name.strip_suffix("Packet").unwrap_or(&packet.name);
        // lowercase the last character of name
        let name = {
            let mut chars = name.chars();
            let last_char = chars.next_back().unwrap();
            let last_char = last_char.to_lowercase().to_string();
            let mut name = chars.collect::<String>();
            name.push_str(&last_char);
            name
        };

        // if the packet is clientbound, but the name does not ends with S2c, add it
        let name = if packet.side == "clientbound" && !name.ends_with("S2c") {
            format!("{name}S2c")
        } else {
            name
        };

        // same for serverbound
        let name = if packet.side == "serverbound" && !name.ends_with("C2s") {
            format!("{name}C2s")
        } else {
            name
        };

        let id = packet.id;
        let side = match packet.side.as_str() {
            "clientbound" => quote! { valence_protocol::PacketSide::Clientbound },
            "serverbound" => quote! { valence_protocol::PacketSide::Serverbound },
            _ => unreachable!(),
        };

        let phase = match packet.phase.as_str() {
            "handshake" => quote! { valence_protocol::PacketState::Handshake },
            "configuration" => quote! { valence_protocol::PacketState::Configuration },
            "status" => quote! { valence_protocol::PacketState::Status },
            "login" => quote! { valence_protocol::PacketState::Login },
            "play" => quote! { valence_protocol::PacketState::Play },
            _ => unreachable!(),
        };

        // const STD_PACKETS =
        // [PacketSide::Client(PacketState::Handshaking(Packet{..})), ..];
        p.push(quote! {
            crate::packet_registry::Packet {
                id: #id,
                side: #side,
                state: #phase,
                timestamp: None,
                name: #name,
                data: None,
            }
        });
    }

    consts.extend([quote! {
        pub const STD_PACKETS: [crate::packet_registry::Packet; #len] = [
            #(#p),*
        ];
    }]);

    write_generated_file(consts, "packets.rs")?;

    Ok(())
}

fn write_transformer(packets: &[Packet]) -> anyhow::Result<()> {
    // HashMap<side, HashMap<state, Vec<name>>>
    let grouped_packets = HashMap::<String, HashMap<String, Vec<String>>>::new();

    let mut grouped_packets = packets.iter().fold(grouped_packets, |mut acc, packet| {
        let side = match packet.side.as_str() {
            "serverbound" => "Serverbound".to_owned(),
            "clientbound" => "Clientbound".to_owned(),
            _ => panic!("Invalid side"),
        };

        let state = match packet.phase.as_str() {
            "handshake" => "Handshake".to_owned(),
            "configuration" => "Configuration".to_owned(),
            "status" => "Status".to_owned(),
            "login" => "Login".to_owned(),
            "play" => "Play".to_owned(),
            _ => panic!("Invalid state"),
        };

        let name = packet
            .name
            .strip_suffix("Packet")
            .unwrap_or(&packet.name)
            .to_owned();

        // lowercase the last character of name
        let name = {
            let mut chars = name.chars();
            let last_char = chars.next_back().unwrap();
            let last_char = last_char.to_lowercase().to_string();
            let mut name = chars.collect::<String>();
            name.push_str(&last_char);
            name
        };

        // if the packet is clientbound, but the name does not ends with S2c, add it
        let name = if side == "Clientbound" && !name.ends_with("S2c") {
            format!("{name}S2c")
        } else {
            name
        };

        // same for serverbound
        let name = if side == "Serverbound" && !name.ends_with("C2s") {
            format!("{name}C2s")
        } else {
            name
        };

        let state_map = acc.entry(side).or_default();
        let id_map = state_map.entry(state).or_default();
        id_map.push(name);

        acc
    });

    let mut generated = TokenStream::new();

    for (side, state_map) in &mut grouped_packets {
        let mut side_arms = TokenStream::new();
        for (state, id_map) in state_map.iter_mut() {
            let mut match_arms = TokenStream::new();

            let lowercase_state = state.to_lowercase();
            let state = syn::parse_str::<syn::Ident>(state).unwrap();
            let lowercase_state = syn::parse_str::<syn::Ident>(&lowercase_state).unwrap();
            for name in id_map.iter_mut() {
                let name = name.to_pascal_case();
                let name = syn::parse_str::<syn::Ident>(&name).unwrap();

                match_arms.extend(quote! {
                    valence_protocol::packets::#lowercase_state::#name::ID => {
                        Ok(format!("{:#?}", valence_protocol::packets::#lowercase_state::#name::decode(&mut data)?))
                    }
                });
            }

            side_arms.extend(quote! {
                valence_protocol::PacketState::#state => match packet.id {
                    #match_arms
                    _ => Ok(NOT_AVAILABLE.to_owned()),
                },
            });
        }

        if side == "Clientbound" {
            side_arms.extend(quote! {
                _ => Ok(NOT_AVAILABLE.to_owned()),
            });
        }

        let side = syn::parse_str::<syn::Ident>(side).unwrap();

        generated.extend(quote! {
            valence_protocol::PacketSide::#side => match packet.state {
                #side_arms
            },
        });
    }

    // wrap generated in a function definition
    let generated = quote! {
        const NOT_AVAILABLE: &str = "Not yet implemented";

        #[allow(clippy::match_wildcard_for_single_variants)]
        pub(crate) fn packet_to_string(packet: &ProxyPacket) -> Result<String, Box<dyn std::error::Error>> {
            let bytes = packet.data.as_ref().unwrap();
            let mut data = &bytes.clone()[..];

            match packet.side {
                #generated
            }
        }
    };

    write_generated_file(generated, "packet_to_string.rs")?;

    Ok(())
}
