use experience_ir::{
    CalendarEvent, ExperienceModel, Music, NetworkState, Note, SystemState, Weather, WifiNetwork,
    WifiSecurity,
};

pub mod state_service;

pub fn snapshot() -> ExperienceModel {
    ExperienceModel {
        greeting: "Good afternoon, Carli".into(),
        date: "Saturday, 8 August".into(),
        weather: Weather {
            summary: "Clear over Rome".into(),
            temperature_c: 28,
            high_c: 31,
            low_c: 21,
        },
        calendar: vec![
            CalendarEvent {
                time: "09:30".into(),
                title: "Design review".into(),
                detail: "SOS interaction model".into(),
            },
            CalendarEvent {
                time: "13:00".into(),
                title: "Lunch with Marta".into(),
                detail: "Piazza Testaccio".into(),
            },
            CalendarEvent {
                time: "18:45".into(),
                title: "Evening run".into(),
                detail: "Villa Borghese · 6 km".into(),
            },
        ],
        notes: vec![
            Note {
                title: "Interface thought".into(),
                preview: "The experience is the program, not a grid of apps.".into(),
            },
            Note {
                title: "Tonight".into(),
                preview: "Book the train and call Luca.".into(),
            },
        ],
        music: Music {
            title: "A Walk".into(),
            artist: "Tycho".into(),
            playing: true,
        },
        system: SystemState {
            unix_time_ms: 0,
            timezone: "UTC".into(),
            online_interfaces: vec!["synthetic0".into()],
            ..SystemState::default()
        },
        surfaces: Vec::new(),
        agent: Default::default(),
        network: NetworkState {
            wifi_enabled: true,
            connected: true,
            connected_ssid: Some("SOS Lab".into()),
            validated: true,
            signal_level: Some(4),
            networks: vec![
                WifiNetwork {
                    ssid: "SOS Lab".into(),
                    signal_level: 4,
                    security: WifiSecurity::Personal,
                    saved: true,
                },
                WifiNetwork {
                    ssid: "Guest".into(),
                    signal_level: 3,
                    security: WifiSecurity::Open,
                    saved: false,
                },
            ],
            activity: "Connected".into(),
            error: None,
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn snapshot_has_every_seeded_domain() {
        let model = super::snapshot();
        assert!(!model.calendar.is_empty());
        assert!(!model.notes.is_empty());
        assert!(!model.music.title.is_empty());
        assert!(!model.weather.summary.is_empty());
        assert!(model.network.connected);
        assert_eq!(model.network.networks.len(), 2);
    }
}
