use std::io::Cursor;

use crate::format::{self, BlockKind, Manifest, DIGEST_BYTES, HEADER_BYTES, NODE_BYTES};
use crate::{
    write_archive, ArchiveError, ArchiveLimits, ArchiveSource, EncodeOptions, NamedRoot, Node,
    OperationControl, ProducerSeal, UnverifiedArchive, EMPTY, UNIT,
};

struct Family {
    atoms: Vec<Vec<u8>>,
    nodes: Vec<Node>,
}

impl ArchiveSource for Family {
    fn variable_count(&self) -> u32 {
        self.atoms.len() as u32
    }
    fn node_count(&self) -> u64 {
        self.nodes.len() as u64
    }

    fn copy_atom(&mut self, variable: u32, output: &mut Vec<u8>) -> Result<(), ArchiveError> {
        let atom = self
            .atoms
            .get(variable as usize)
            .ok_or(ArchiveError::Invalid("fixture atom"))?;
        output
            .try_reserve(atom.len())
            .map_err(|_| ArchiveError::Allocation)?;
        output.extend_from_slice(atom);
        Ok(())
    }

    fn node(&mut self, id: u64) -> Result<Node, ArchiveError> {
        self.nodes
            .get((id - 2) as usize)
            .copied()
            .ok_or(ArchiveError::Invalid("fixture node"))
    }
}

fn family() -> Family {
    Family {
        atoms: vec![b"red".to_vec(), b"blue".to_vec(), vec![0, 0xff, 7]],
        nodes: vec![
            Node {
                variable: 2,
                low: EMPTY,
                high: UNIT,
                count: 1,
            },
            Node {
                variable: 1,
                low: 2,
                high: UNIT,
                count: 2,
            },
            Node {
                variable: 0,
                low: 3,
                high: 2,
                count: 3,
            },
        ],
    }
}

fn roots() -> Vec<NamedRoot> {
    [("a", 4), ("b", 3), ("c", 2), ("d", EMPTY), ("e", UNIT)]
        .into_iter()
        .map(|(name, node)| NamedRoot {
            name: name.to_owned(),
            node,
        })
        .collect()
}

fn options() -> EncodeOptions {
    EncodeOptions {
        atom_block_items: 1,
        node_block_items: 1,
    }
}

fn encode(source: &mut Family) -> (Vec<u8>, ProducerSeal) {
    let mut output = Cursor::new(Vec::new());
    let seal = write_archive(
        source,
        &roots(),
        b"opaque application bytes",
        &mut output,
        options(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    (output.into_inner(), seal)
}

fn manifest(bytes: &[u8]) -> Manifest {
    let header = format::header(&bytes[..HEADER_BYTES], ArchiveLimits::default()).unwrap();
    let end = header.manifest_bytes as usize;
    let digest = bytes[end..end + DIGEST_BYTES].try_into().unwrap();
    format::decode_manifest(&bytes[..end], digest, ArchiveLimits::default()).unwrap()
}

fn rewrite_manifest(bytes: &mut [u8], manifest: &Manifest) {
    let encoded = format::encode_manifest(manifest).unwrap();
    let digest = format::digest(&encoded);
    bytes[..encoded.len()].copy_from_slice(&encoded);
    bytes[encoded.len()..encoded.len() + DIGEST_BYTES].copy_from_slice(&digest);
}

fn rewrite_node(bytes: &mut [u8], id: u64, modify: impl FnOnce(&mut Node)) {
    let mut manifest = manifest(bytes);
    let block = manifest
        .blocks
        .iter_mut()
        .find(|block| {
            block.kind == BlockKind::Nodes
                && id >= block.first
                && id < block.first + u64::from(block.items)
        })
        .unwrap();
    let at = block.offset as usize + (id - block.first) as usize * NODE_BYTES;
    let mut node =
        format::decode_node(&bytes[at..at + NODE_BYTES], id, manifest.variables).unwrap();
    modify(&mut node);
    let mut encoded = Vec::new();
    format::encode_node(node, &mut encoded);
    bytes[at..at + NODE_BYTES].copy_from_slice(&encoded);
    block.digest =
        format::digest(&bytes[block.offset as usize..block.offset as usize + block.bytes as usize]);
    rewrite_manifest(bytes, &manifest);
}

#[test]
fn shared_roots_opaque_atoms_counts_membership_and_rank_pages_roundtrip() {
    let (bytes, seal) = encode(&mut family());
    let unverified = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    assert_eq!(
        unverified.metrics().block_fetches,
        0,
        "opening fetched only the manifest"
    );
    let mut reader = unverified.accept_producer_seal(&seal).unwrap();
    assert!(!reader.full_scan_performed());
    assert_eq!(reader.count(0).unwrap(), 3);
    assert_eq!(reader.count(1).unwrap(), 2);
    assert_eq!(reader.count(2).unwrap(), 1);
    assert_eq!(reader.count(3).unwrap(), 0);
    assert_eq!(reader.count(4).unwrap(), 1);
    assert_eq!(
        reader
            .page(0, 0, 3, OperationControl::default())
            .unwrap()
            .sets,
        vec![vec![2], vec![1], vec![0, 2]]
    );
    assert_eq!(
        reader
            .page(0, 2, 1, OperationControl::default())
            .unwrap()
            .sets,
        vec![vec![0, 2]]
    );
    assert!(reader
        .page(0, u128::MAX, 1, OperationControl::default())
        .unwrap()
        .sets
        .is_empty());
    assert_eq!(
        reader.atom(2, OperationControl::default()).unwrap(),
        vec![0, 0xff, 7]
    );
    assert!(reader
        .contains(0, &[0, 2], OperationControl::default())
        .unwrap());
    assert!(!reader
        .contains(0, &[0], OperationControl::default())
        .unwrap());
    assert!(reader
        .contains(4, &[], OperationControl::default())
        .unwrap());
    assert!(!reader
        .contains(3, &[], OperationControl::default())
        .unwrap());
    assert!(reader
        .contains(0, &[2, 0], OperationControl::default())
        .is_err());
    assert!(reader
        .contains(0, &[3], OperationControl::default())
        .is_err());
    assert!(reader.info().metadata == b"opaque application bytes");
}

#[test]
fn unknown_archive_needs_a_scan_and_scan_does_not_expand_sets() {
    let (bytes, _) = encode(&mut family());
    let reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    let reader = reader.verify_all(OperationControl::default()).unwrap();
    assert!(reader.full_scan_performed());
    assert_eq!(reader.count(0).unwrap(), 3);
    assert!(reader.metrics().block_fetches <= reader.info().block_count as u64);
}

#[test]
fn lazy_page_does_not_fetch_atom_or_unrelated_root_blocks() {
    let (bytes, seal) = encode(&mut family());
    let mut reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    assert_eq!(
        reader
            .page(2, 0, 1, OperationControl::default())
            .unwrap()
            .sets,
        vec![vec![2]]
    );
    assert_eq!(reader.metrics().block_fetches, 1);
    assert_eq!(
        reader.atom(2, OperationControl::default()).unwrap(),
        vec![0, 0xff, 7]
    );
    assert_eq!(reader.metrics().block_fetches, 2);
}

#[test]
fn producer_seal_is_bound_to_the_identical_manifest_and_directory() {
    let (a, seal) = encode(&mut family());
    let mut changed = family();
    changed.atoms[0] = b"different atom".to_vec();
    let (b, _) = encode(&mut changed);
    assert_ne!(a, b);
    let reader = UnverifiedArchive::open(
        b.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    assert!(matches!(
        reader.accept_producer_seal(&seal),
        Err(ArchiveError::SealMismatch)
    ));
}

#[test]
fn tampered_lazy_blocks_fail_even_with_a_valid_original_producer_seal() {
    let (mut bytes, seal) = encode(&mut family());
    let directory = manifest(&bytes);
    let block = &directory.blocks[0];
    bytes[block.offset as usize + 4] ^= 1;
    let mut reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    assert!(matches!(
        reader.atom(0, OperationControl::default()),
        Err(ArchiveError::ChecksumMismatch)
    ));
}

#[test]
fn checksums_do_not_authorize_incorrect_counts() {
    let (mut bytes, original_seal) = encode(&mut family());
    rewrite_node(&mut bytes, 4, |node| node.count = 4);
    let mut directory = manifest(&bytes);
    directory.roots[0].declared_count = 4;
    rewrite_manifest(&mut bytes, &directory);
    let reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    assert!(matches!(
        reader.accept_producer_seal(&original_seal),
        Err(ArchiveError::SealMismatch)
    ));
    let reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    assert!(matches!(
        reader.verify_all(OperationControl::default()),
        Err(ArchiveError::Invalid("node count recurrence"))
    ));
}

#[test]
fn writer_checks_actual_serialized_counts_before_issuing_a_seal() {
    let mut source = family();
    source.nodes[2].count = 4;
    let mut output = Cursor::new(Vec::new());
    assert!(matches!(
        write_archive(
            &mut source,
            &roots(),
            &[],
            &mut output,
            options(),
            ArchiveLimits::default(),
            OperationControl::default()
        ),
        Err(ArchiveError::Invalid("node count recurrence"))
    ));
}

#[test]
fn cycles_and_non_increasing_variable_order_are_rejected() {
    for mutate in [
        (|node: &mut Node| node.high = 4) as fn(&mut Node),
        |node: &mut Node| node.variable = 1,
        |node: &mut Node| node.high = EMPTY,
    ] {
        let (mut bytes, _) = encode(&mut family());
        rewrite_node(&mut bytes, 4, mutate);
        let reader = UnverifiedArchive::open(
            bytes.as_slice(),
            ArchiveLimits::default(),
            OperationControl::default(),
        )
        .unwrap();
        assert!(matches!(
            reader.verify_all(OperationControl::default()),
            Err(ArchiveError::Invalid(_))
        ));
    }
}

#[test]
fn root_counts_and_directory_extents_are_not_trusted() {
    let (mut bytes, _) = encode(&mut family());
    let mut directory = manifest(&bytes);
    directory.roots[0].declared_count += 1;
    rewrite_manifest(&mut bytes, &directory);
    let reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    assert!(matches!(
        reader.verify_all(OperationControl::default()),
        Err(ArchiveError::Invalid("root count"))
    ));
    let (mut bytes, _) = encode(&mut family());
    let mut directory = manifest(&bytes);
    directory.blocks[1].offset = directory.blocks[0].offset;
    rewrite_manifest(&mut bytes, &directory);
    assert!(matches!(
        UnverifiedArchive::open(
            bytes.as_slice(),
            ArchiveLimits::default(),
            OperationControl::default()
        ),
        Err(ArchiveError::Invalid(_))
    ));
}

#[test]
fn trailing_truncated_and_bad_manifest_bytes_are_rejected() {
    let (bytes, _) = encode(&mut family());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(UnverifiedArchive::open(
        trailing.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default()
    )
    .is_err());
    assert!(UnverifiedArchive::open(
        &bytes[..bytes.len() - 1],
        ArchiveLimits::default(),
        OperationControl::default()
    )
    .is_err());
    let mut corrupt = bytes;
    corrupt[HEADER_BYTES] ^= 1;
    assert!(matches!(
        UnverifiedArchive::open(
            corrupt.as_slice(),
            ArchiveLimits::default(),
            OperationControl::default()
        ),
        Err(ArchiveError::ChecksumMismatch)
    ));
}

fn powerset(variables: u32) -> Family {
    let mut nodes = Vec::new();
    for index in 0..variables {
        let child = if index == 0 {
            UNIT
        } else {
            u64::from(index) + 1
        };
        let count = if index < 127 {
            1_u128 << (index + 1)
        } else {
            u128::MAX
        };
        nodes.push(Node {
            variable: variables - index - 1,
            low: child,
            high: child,
            count,
        });
    }
    Family {
        atoms: vec![vec![]; variables as usize],
        nodes,
    }
}

#[test]
fn equal_low_high_is_valid_zdd_and_huge_offsets_skip_counted_branches() {
    let mut source = powerset(50);
    let roots = [NamedRoot {
        name: "power".into(),
        node: 51,
    }];
    let mut output = Cursor::new(Vec::new());
    let seal = write_archive(
        &mut source,
        &roots,
        &[],
        &mut output,
        EncodeOptions::default(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    let bytes = output.into_inner();
    let mut reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    assert_eq!(reader.count(0).unwrap(), 1_u128 << 50);
    let page = reader
        .page(
            0,
            (1_u128 << 50) - 1,
            1,
            OperationControl {
                max_work: 1000,
                cancellation: None,
            },
        )
        .unwrap();
    assert_eq!(page.sets, vec![(0..50).collect::<Vec<_>>()]);
    assert_eq!(reader.metrics().block_fetches, 1);
}

#[test]
fn exact_u128_overflow_is_refused_without_saturation() {
    let mut source = powerset(128);
    let roots = [NamedRoot {
        name: "overflow".into(),
        node: 129,
    }];
    let mut output = Cursor::new(Vec::new());
    assert!(matches!(
        write_archive(
            &mut source,
            &roots,
            &[],
            &mut output,
            EncodeOptions::default(),
            ArchiveLimits::default(),
            OperationControl::default()
        ),
        Err(ArchiveError::CountOverflow)
    ));
}

#[test]
fn cancellation_and_explicit_work_limits_never_return_partial_success() {
    let (bytes, _) = encode(&mut family());
    let cancel = || true;
    assert!(matches!(
        UnverifiedArchive::open(
            bytes.as_slice(),
            ArchiveLimits::default(),
            OperationControl {
                max_work: 0,
                cancellation: Some(&cancel)
            }
        ),
        Err(ArchiveError::Cancelled)
    ));
    let reader = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    assert!(matches!(
        reader.verify_all(OperationControl {
            max_work: 1,
            cancellation: None
        }),
        Err(ArchiveError::WorkLimit)
    ));
}

#[test]
fn representation_and_page_limits_refuse_before_unbounded_output() {
    let (bytes, seal) = encode(&mut family());
    let limits = ArchiveLimits {
        max_manifest_bytes: HEADER_BYTES,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        UnverifiedArchive::open(bytes.as_slice(), limits, OperationControl::default()),
        Err(ArchiveError::Limit("manifest bytes"))
    ));
    let limits = ArchiveLimits {
        max_nodes: 2,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        UnverifiedArchive::open(bytes.as_slice(), limits, OperationControl::default()),
        Err(ArchiveError::Limit("nodes"))
    ));
    let limits = ArchiveLimits {
        max_page_variables: 1,
        ..ArchiveLimits::default()
    };
    let mut reader = UnverifiedArchive::open(bytes.as_slice(), limits, OperationControl::default())
        .unwrap()
        .accept_producer_seal(&seal)
        .unwrap();
    assert!(matches!(
        reader.page(0, 0, 3, OperationControl::default()),
        Err(ArchiveError::Limit("page variables"))
    ));
}

#[test]
fn tiny_and_disabled_block_caches_preserve_exact_paging() {
    let (bytes, seal) = encode(&mut family());
    for cache_bytes in [0, 1, 128] {
        let limits = ArchiveLimits {
            cache_bytes,
            max_cached_blocks: 1,
            ..ArchiveLimits::default()
        };
        let mut reader =
            UnverifiedArchive::open(bytes.as_slice(), limits, OperationControl::default())
                .unwrap()
                .accept_producer_seal(&seal)
                .unwrap();
        assert_eq!(
            reader
                .page(0, 0, 3, OperationControl::default())
                .unwrap()
                .sets,
            vec![vec![2], vec![1], vec![0, 2]]
        );
        assert!(reader.metrics().retained_cache_bytes <= cache_bytes);
        reader.clear_cache();
        assert_eq!(reader.metrics().retained_cache_bytes, 0);
        assert_eq!(reader.metrics().uncached_block_bytes, 0);
    }
}

#[test]
fn deep_singleton_pages_schedule_only_intersecting_branches() {
    let variables = 4096_u32;
    let mut source = Family {
        atoms: (0..variables).map(|_| Vec::new()).collect(),
        nodes: (0..variables)
            .map(|index| Node {
                variable: variables - index - 1,
                low: if index == 0 {
                    EMPTY
                } else {
                    u64::from(index) + 1
                },
                high: UNIT,
                count: u128::from(index) + 1,
            })
            .collect(),
    };
    let mut output = Cursor::new(Vec::new());
    let seal = write_archive(
        &mut source,
        &[NamedRoot {
            name: "singletons".to_owned(),
            node: u64::from(variables) + 1,
        }],
        b"",
        &mut output,
        EncodeOptions::default(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap();
    let mut reader = UnverifiedArchive::open(
        output.get_ref().as_slice(),
        ArchiveLimits {
            max_page_variables: 3,
            ..ArchiveLimits::default()
        },
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    assert_eq!(
        reader
            .page(0, 0, 1, OperationControl::default())
            .unwrap()
            .sets,
        vec![vec![variables - 1]],
    );
    assert_eq!(reader.metrics().peak_page_stack_entries, 1);
    assert_eq!(
        reader
            .page(0, 2047, 3, OperationControl::default())
            .unwrap()
            .sets,
        vec![vec![2048], vec![2047], vec![2046]],
    );
    assert!(reader.metrics().peak_page_stack_entries <= 3);
    assert_eq!(
        reader
            .page(0, u128::from(variables - 1), 1, OperationControl::default())
            .unwrap()
            .sets,
        vec![vec![0]],
    );

    let (bytes, seal) = encode(&mut family());
    let mut prefix_limited = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits {
            max_page_variables: 1,
            ..ArchiveLimits::default()
        },
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    assert!(matches!(
        prefix_limited.page(0, 2, 1, OperationControl::default()),
        Err(ArchiveError::Limit("page variables")),
    ));
}
