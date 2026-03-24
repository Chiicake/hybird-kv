use hkv_server::phase2a_testing::{
    CommandKind, ExactHotKey, ExactHotnessEvaluator, ObservationEvent,
};

#[test]
fn exact_ground_truth_repeated_hot_key_pattern_counts_every_access() {
    let ranking = ExactHotnessEvaluator::from_events(vec![
        observed_read(CommandKind::Get, b"alpha"),
        observed_read(CommandKind::Get, b"alpha"),
        observed_read(CommandKind::Get, b"alpha"),
        observed_read(CommandKind::Get, b"beta"),
    ])
    .top_keys(2);

    assert_eq!(
        ranking,
        vec![
            ExactHotKey {
                key: b"alpha".to_vec(),
                total_accesses: 3,
                read_accesses: 3,
                write_accesses: 0,
            },
            ExactHotKey {
                key: b"beta".to_vec(),
                total_accesses: 1,
                read_accesses: 1,
                write_accesses: 0,
            },
        ]
    );
}

#[test]
fn exact_ground_truth_mixed_read_write_access_keeps_split_counts() {
    let ranking = ExactHotnessEvaluator::from_events(vec![
        observed_write(CommandKind::Set, b"alpha", Some(5)),
        observed_read(CommandKind::Get, b"alpha"),
        observed_write(CommandKind::Expire, b"alpha", None),
        observed_write(CommandKind::Delete, b"beta", None),
        observed_read(CommandKind::Ttl, b"alpha"),
    ])
    .top_keys(2);

    assert_eq!(
        ranking,
        vec![
            ExactHotKey {
                key: b"alpha".to_vec(),
                total_accesses: 4,
                read_accesses: 2,
                write_accesses: 2,
            },
            ExactHotKey {
                key: b"beta".to_vec(),
                total_accesses: 1,
                read_accesses: 0,
                write_accesses: 1,
            },
        ]
    );
}

#[test]
fn exact_ground_truth_multiple_keys_preserves_stable_heavy_hitters() {
    let ranking = ExactHotnessEvaluator::from_events(vec![
        observed_read(CommandKind::Get, b"hot-a"),
        observed_read(CommandKind::Get, b"hot-b"),
        observed_write(CommandKind::Set, b"hot-a", Some(3)),
        observed_read(CommandKind::Get, b"cold"),
        observed_read(CommandKind::Ttl, b"hot-b"),
        observed_write(CommandKind::Expire, b"hot-a", None),
        observed_write(CommandKind::Delete, b"hot-b", None),
        observed_read(CommandKind::Get, b"hot-a"),
    ])
    .top_keys(3);

    assert_eq!(
        ranking,
        vec![
            ExactHotKey {
                key: b"hot-a".to_vec(),
                total_accesses: 4,
                read_accesses: 2,
                write_accesses: 2,
            },
            ExactHotKey {
                key: b"hot-b".to_vec(),
                total_accesses: 3,
                read_accesses: 2,
                write_accesses: 1,
            },
            ExactHotKey {
                key: b"cold".to_vec(),
                total_accesses: 1,
                read_accesses: 1,
                write_accesses: 0,
            },
        ]
    );
}

fn observed_read(command: CommandKind, key: &[u8]) -> ObservationEvent {
    ObservationEvent::read(command, key.to_vec(), std::time::UNIX_EPOCH)
}

fn observed_write(command: CommandKind, key: &[u8], value_size: Option<usize>) -> ObservationEvent {
    ObservationEvent::write(command, key.to_vec(), value_size, std::time::UNIX_EPOCH)
}
