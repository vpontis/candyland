use concurrent_merkle_tree::error::CMTError;
use concurrent_merkle_tree::merkle_roll::{MerkleRoll, MerkleInterface};
use concurrent_merkle_tree::state::{Node, EMPTY};
use concurrent_merkle_tree::utils::{recompute, hash_to_parent};
use merkle_tree_reference::MerkleTree;
use rand::thread_rng;
use rand::{self, Rng};
use tokio;

const DEPTH: usize = 14;
const BUFFER_SIZE: usize = 64;

fn make_empty_offchain_tree_of_depth(depth: usize) -> MerkleTree {
    let mut leaves = vec![];
    for _ in 0..(1 << depth) {
        let leaf = EMPTY;
        leaves.push(leaf);
    }

    MerkleTree::new(leaves)
}

fn setup() -> (MerkleRoll<DEPTH, BUFFER_SIZE>, MerkleTree) {
    // On-chain merkle change-record
    let merkle = MerkleRoll::<DEPTH, BUFFER_SIZE>::new();

    // Init off-chain Merkle tree with corresponding # of leaves
    let reference_tree = make_empty_offchain_tree_of_depth(DEPTH);

    (merkle, reference_tree)
}

#[tokio::test(threaded_scheduler)]
async fn test_initialize() {
    let (mut merkle_roll, off_chain_tree) = setup();
    merkle_roll.initialize().unwrap();

    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "Init failed to set root properly"
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_append() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    for i in 0..(1 << DEPTH) {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
        assert_eq!(
            merkle_roll.get_change_log().get_root(),
            off_chain_tree.get_root(),
            "On chain tree failed to update properly on an append",
        );
    }

    assert_eq!(
        merkle_roll.buffer_size, BUFFER_SIZE as u64,
        "Merkle roll buffer size is wrong"
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_append_complete_subtree() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // insert eight leaves into the large tree
    for i in 0..8 {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
        assert_eq!(
            merkle_roll.get_change_log().get_root(),
            off_chain_tree.get_root(),
            "On chain tree failed to update properly on an append",
        );
    }

    // create a simple subtree to append of depth three
    let mut onchain_subtree = MerkleRoll::<3, 8>::new();
    onchain_subtree.initialize().unwrap();

    // completely fill the subtree with unique leaves, and also append them to the off-chain tree
    for i in 8..16 {
        let leaf = rng.gen::<[u8; 32]>();
        onchain_subtree.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
    }

    // append the on_chain subtree to the merkle_roll
    merkle_roll
        .append_subtree_direct(
            onchain_subtree.get_change_log().get_root(),
            onchain_subtree.rightmost_proof.leaf,
            onchain_subtree.rightmost_proof.index,
            &onchain_subtree.rightmost_proof.proof.to_vec(),
        )
        .unwrap();

    // The result should be that the merkle_roll's new root is the same as the root of the off-chain tree which had leaves 0..15 appended one by one
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "On chain tree failed to update properly on an append",
    );

    // Show that we can still append to the large tree after performing a subtree append
    let leaf = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf).unwrap();
    off_chain_tree.add_leaf(leaf, 16);
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "Failed to append accurately to merkle roll after subtree append",
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_append_incomplete_subtree() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // insert four leaves into the large tree
    for i in 0..4 {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
        assert_eq!(
            merkle_roll.get_change_log().get_root(),
            off_chain_tree.get_root(),
            "On chain tree failed to update properly on an append",
        );
    }

    // create a simple subtree to append of depth three
    let mut onchain_subtree = MerkleRoll::<2, 8>::new();
    onchain_subtree.initialize().unwrap();

    // append leaves to the subtree, and also append them to the off-chain tree
    // note: this gives us a partially filled tree, the other two leaves are empty nodes
    for i in 4..6 {
        let leaf = rng.gen::<[u8; 32]>();
        onchain_subtree.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
    }

    // append the on_chain subtree to the merkle_roll
    merkle_roll
        .append_subtree_direct(
            onchain_subtree.get_change_log().get_root(),
            onchain_subtree.rightmost_proof.leaf,
            onchain_subtree.rightmost_proof.index,
            &onchain_subtree.rightmost_proof.proof.to_vec(),
        )
        .unwrap();

    // The result should be that the merkle_roll's new root is the same as the root of the off-chain tree which had leaves 0..5 appended one by one
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "On chain tree failed to update properly on an append",
    );

    // Show that we can still append to the large tree after performing a subtree append
    let leaf = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf).unwrap();
    off_chain_tree.add_leaf(leaf, 6);
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "Failed to append accurately to merkle roll after subtree append",
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_append_subtree_to_empty_tree() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // create a simple subtree to append of depth two
    let mut onchain_subtree = MerkleRoll::<2, 8>::new();
    onchain_subtree.initialize().unwrap();

    // append leaves to the subtree, and also append them to the off-chain tree
    for i in 0..4 {
        let leaf = rng.gen::<[u8; 32]>();
        onchain_subtree.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
    }

    // append the on_chain subtree to the merkle_roll
    merkle_roll
        .append_subtree_direct(
            onchain_subtree.get_change_log().get_root(),
            onchain_subtree.rightmost_proof.leaf,
            onchain_subtree.rightmost_proof.index,
            &onchain_subtree.rightmost_proof.proof.to_vec(),
        )
        .unwrap();

    // The result should be that the merkle_roll's new root is the same as the root of the off-chain tree which had leaves 0..4 appended one by one
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "On chain tree failed to update properly on an append",
    );

    // Show that we can still append to the large tree after performing a subtree append
    let leaf = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf).unwrap();
    off_chain_tree.add_leaf(leaf, 4);
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "Failed to append accurately to merkle roll after subtree append",
    );
}

// Working on this, lets make it simpler
#[tokio::test(threaded_scheduler)]
async fn test_append_complete_subtree_tightly_packed_depth_three() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // insert one leaf into the larger tree
    for i in 0..1 {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
        assert_eq!(
            merkle_roll.get_change_log().get_root(),
            off_chain_tree.get_root(),
            "Large tree failed to update properly on an append",
        );
    }

    let (mut small_off_chain_tree) = make_empty_offchain_tree_of_depth(3);

    // completely fill the subtree with unique leaves, and also append them to the off-chain tree
    let mut leaves_in_small_tree = vec![];
    for i in 0..8 {
        let leaf = rng.gen::<[u8; 32]>();
        small_off_chain_tree.add_leaf(leaf, i);
        leaves_in_small_tree.push(leaf);
    }

    // The order of the leaves will be shuffled around based on the tight append algo. The order below was manually calculated based on the test case situation
    off_chain_tree.add_leaf(leaves_in_small_tree[6], 1);
    off_chain_tree.add_leaf(leaves_in_small_tree[4], 2);
    off_chain_tree.add_leaf(leaves_in_small_tree[5], 3);
    off_chain_tree.add_leaf(leaves_in_small_tree[0], 4);
    off_chain_tree.add_leaf(leaves_in_small_tree[1], 5);
    off_chain_tree.add_leaf(leaves_in_small_tree[2], 6);
    off_chain_tree.add_leaf(leaves_in_small_tree[3], 7);
    off_chain_tree.add_leaf(leaves_in_small_tree[7], 8);

    // Mock the creation of the pre-append data structure
    let subtree_proofs: Vec<Vec<Node>>  = vec![vec![], vec![], vec![small_off_chain_tree.get_node(4)], vec![small_off_chain_tree.get_node(2), small_off_chain_tree.get_proof_of_leaf(3)[1]]];
    let subtree_rmls: Vec<Node> = vec![small_off_chain_tree.get_node(7), small_off_chain_tree.get_node(6), small_off_chain_tree.get_node(5), small_off_chain_tree.get_node(3)];
    let subtree_roots: Vec<Node> = vec![small_off_chain_tree.get_node(7), small_off_chain_tree.get_node(6), small_off_chain_tree.get_proof_of_leaf(7)[1], small_off_chain_tree.get_proof_of_leaf(7)[2]];

    // append the small_subtree to merkle_roll
    merkle_roll
        .append_subtree_packed(
            &subtree_proofs,
            &subtree_rmls,
            &subtree_roots
        )
        .unwrap();

    // The result should be that the merkle_roll's new root is the same as the root of the off-chain tree which had leaves 0..9 appended one by one
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "On chain tree failed to update properly on an append",
    );

    // The index of the rmp to the on_chain_merkle roll should be equivalent to us having performed nine appends, since the append should be dense
    assert_eq!(
        merkle_roll.rightmost_proof.index,
        9,
        "On chain append was not tightly packed"
    );

    // Show that we can still append to the large tree after performing a subtree append
    let leaf = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf).unwrap();
    off_chain_tree.add_leaf(leaf, 9);
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "Failed to append accurately to merkle roll after subtree append",
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_append_complete_subtree_tightly_packed_depth_one() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // insert one leaf into the larger tree
    for i in 0..1 {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
        assert_eq!(
            merkle_roll.get_change_log().get_root(),
            off_chain_tree.get_root(),
            "Large tree failed to update properly on an append",
        );
    }

    let (mut small_off_chain_tree) = make_empty_offchain_tree_of_depth(1);

    // completely fill the subtree with unique leaves
    let mut leaves_in_small_tree = vec![];
    for i in 0..2 {
        let leaf = rng.gen::<[u8; 32]>();
        small_off_chain_tree.add_leaf(leaf, i);
        leaves_in_small_tree.push(leaf);
    }

    // Append the leaves of the subtree to the bigger offchain tree in the order that the tightly packed algo will apply them
    off_chain_tree.add_leaf(leaves_in_small_tree[1], 1);
    off_chain_tree.add_leaf(leaves_in_small_tree[0], 2);

    // Mock the creation of the pre-append data structure
    let subtree_proofs: Vec<Vec<Node>>  = vec![vec![], vec![]];
    let subtree_rmls: Vec<Node> = vec![small_off_chain_tree.get_node(0), small_off_chain_tree.get_node(1)];
    let subtree_roots: Vec<Node> = vec![small_off_chain_tree.get_node(0), small_off_chain_tree.get_node(1)];

    // append the small_merkle_roll to merkle_roll
    merkle_roll
        .append_subtree_packed(
            &subtree_proofs,
            &subtree_rmls,
            &subtree_roots
        )
        .unwrap();

    // The index of the rmp to the on_chain_merkle roll should be equivalent to us having performed two appends, since the append should be dense
    assert_eq!(
        merkle_roll.rightmost_proof.index,
        3,
        "On chain append was not tightly packed"
    );

    // The result should be that the merkle_roll's new root is the same as the root of the off-chain tree which had leaves 0..15 appended one by one
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "On chain tree failed to update properly on an append",
    );

    // Show that we can still append to the large tree after performing a subtree append
    let leaf = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf).unwrap();
    off_chain_tree.add_leaf(leaf, 3);
    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        off_chain_tree.get_root(),
        "Failed to append accurately to merkle roll after subtree append",
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_prove_leaf() {
    let (mut merkle_roll, mut off_chain_tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    for i in 0..(1 << DEPTH) {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll.append(leaf).unwrap();
        off_chain_tree.add_leaf(leaf, i);
    }

    // Test that all leaves can be verified
    for leaf_index in 0..(1 << DEPTH) {
        merkle_roll
            .prove_leaf(
                off_chain_tree.get_root(),
                off_chain_tree.get_leaf(leaf_index),
                &off_chain_tree.get_proof_of_leaf(leaf_index),
                leaf_index as u32,
            )
            .unwrap();
    }

    // Test that old proofs can be verified
    // Up to BUFFER_SIZE old
    let num_leaves_to_try = 10;
    for _ in 0..num_leaves_to_try {
        let leaf_idx = rng.gen_range(0, 1 << DEPTH);
        let last_leaf_idx = off_chain_tree.leaf_nodes.len() - 1;
        let root = off_chain_tree.get_root();
        let leaf = off_chain_tree.get_leaf(leaf_idx);
        let old_proof = off_chain_tree.get_proof_of_leaf(leaf_idx);

        // While executing random replaces, check
        for _ in 0..BUFFER_SIZE {
            let new_leaf = rng.gen::<Node>();
            let mut random_leaf_idx = rng.gen_range(0, 1 << DEPTH);
            while random_leaf_idx == leaf_idx {
                random_leaf_idx = rng.gen_range(0, 1 << DEPTH);
            }

            merkle_roll
                .set_leaf(
                    off_chain_tree.get_root(),
                    off_chain_tree.get_leaf(random_leaf_idx),
                    new_leaf,
                    &off_chain_tree.get_proof_of_leaf(random_leaf_idx),
                    random_leaf_idx as u32,
                )
                .unwrap();
            off_chain_tree.add_leaf(new_leaf, random_leaf_idx);

            // Assert that we can still prove existence of our mostly unused leaf
            merkle_roll
                .prove_leaf(root, leaf, &old_proof, leaf_idx as u32)
                .unwrap();
        }
    }
}

#[tokio::test(threaded_scheduler)]
async fn test_initialize_with_root() {
    let (mut merkle_roll, mut tree) = setup();
    let mut rng = thread_rng();

    for i in 0..(1 << DEPTH) {
        tree.add_leaf(rng.gen::<[u8; 32]>(), i);
    }

    let last_leaf_idx = tree.leaf_nodes.len() - 1;
    merkle_roll
        .initialize_with_root(
            tree.get_root(),
            tree.get_leaf(last_leaf_idx),
            &tree.get_proof_of_leaf(last_leaf_idx),
            last_leaf_idx as u32,
        )
        .unwrap();

    assert_eq!(
        merkle_roll.get_change_log().get_root(),
        tree.get_root(),
        "Init failed to set root properly"
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_leaf_contents_modified() {
    let (mut merkle_roll, mut tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // Create tree with a single leaf
    let leaf = rng.gen::<[u8; 32]>();
    tree.add_leaf(leaf, 0);
    merkle_roll.append(leaf).unwrap();

    // Save a proof of this leaf
    let root = tree.get_root();
    let proof = tree.get_proof_of_leaf(0);

    // Update leaf to be something else
    let new_leaf_0 = rng.gen::<[u8; 32]>();
    tree.add_leaf(leaf, 0);
    merkle_roll
        .set_leaf(root, leaf, new_leaf_0, &proof, 0 as u32)
        .unwrap();

    // Should fail to replace same leaf using outdated info
    let new_leaf_1 = rng.gen::<[u8; 32]>();
    tree.add_leaf(leaf, 0);
    match merkle_roll.set_leaf(root, leaf, new_leaf_1, &proof, 0 as u32) {
        Ok(_) => {
            assert!(
                false,
                "Merkle roll should fail when replacing leafs with outdated leaf proofs"
            )
        }
        Err(e) => match e {
            CMTError::LeafContentsModified => {}
            _ => {
                // println!()
                assert!(false, "Wrong error was thrown: {:?}", e);
            }
        },
    }
}

#[tokio::test(threaded_scheduler)]
async fn test_replaces() {
    let (mut merkle_roll, mut tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // Fill both trees with random nodes
    for i in 0..(1 << DEPTH) {
        let leaf = rng.gen::<[u8; 32]>();
        tree.add_leaf(leaf, i);
        merkle_roll.append(leaf).unwrap();
    }
    assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());

    // Replace leaves in order
    for i in 0..(1 << DEPTH) {
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll
            .set_leaf(
                tree.get_root(),
                tree.get_leaf(i),
                leaf,
                &tree.get_proof_of_leaf(i),
                i as u32,
            )
            .unwrap();
        tree.add_leaf(leaf, i);
        assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());
    }

    // Replaces leaves in a random order by 4x capacity
    let test_capacity: usize = 4 * (1 << DEPTH);
    for _ in 0..(test_capacity) {
        let index = rng.gen_range(0, test_capacity) % (1 << DEPTH);
        let leaf = rng.gen::<[u8; 32]>();
        merkle_roll
            .set_leaf(
                tree.get_root(),
                tree.get_leaf(index),
                leaf,
                &tree.get_proof_of_leaf(index),
                index as u32,
            )
            .unwrap();
        tree.add_leaf(leaf, index);
        assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());
    }
}

#[tokio::test(threaded_scheduler)]
async fn test_default_node_is_empty() {
    assert_eq!(
        Node::default(),
        EMPTY,
        "Expected default() to be the empty node"
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_mixed() {
    let (mut merkle_roll, mut tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // Fill both trees with random nodes
    let mut tree_size = 10;
    for i in 0..tree_size {
        let leaf = rng.gen::<[u8; 32]>();
        tree.add_leaf(leaf, i);
        merkle_roll.append(leaf).unwrap();
    }
    assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());

    // Replaces leaves in a random order by 4x capacity
    let mut last_rmp = merkle_roll.rightmost_proof;

    let tree_capacity: usize = 1 << DEPTH;
    while tree_size < tree_capacity {
        let leaf = rng.gen::<[u8; 32]>();
        let random_num: u32 = rng.gen_range(0, 10);
        if random_num < 5 {
            println!("{} append", tree_size);
            merkle_roll.append(leaf).unwrap();
            tree.add_leaf(leaf, tree_size);
            tree_size += 1;
        } else {
            let index = rng.gen_range(0, tree_size) % (tree_size);
            println!("{} replace {}", tree_size, index);
            merkle_roll
                .set_leaf(
                    tree.get_root(),
                    tree.get_leaf(index),
                    leaf,
                    &tree.get_proof_of_leaf(index),
                    index as u32,
                )
                .unwrap();
            tree.add_leaf(leaf, index);
        }
        if merkle_roll.get_change_log().get_root() != tree.get_root() {
            let last_active_index: usize =
                (merkle_roll.active_index as usize + BUFFER_SIZE - 1) % BUFFER_SIZE;
        }
        last_rmp = merkle_roll.rightmost_proof;
        assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());
    }
}

#[tokio::test(threaded_scheduler)]
/// Append after replacing the last leaf
async fn test_append_bug_repro_1() {
    let (mut merkle_roll, mut tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // Fill both trees with random nodes
    let mut tree_size = 10;
    for i in 0..tree_size {
        let leaf = rng.gen::<[u8; 32]>();
        tree.add_leaf(leaf, i);
        merkle_roll.append(leaf).unwrap();
    }
    assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());

    // Replace the rightmost leaf
    let leaf_0 = rng.gen::<[u8; 32]>();
    let index = 9;
    merkle_roll
        .set_leaf(
            tree.get_root(),
            tree.get_leaf(index),
            leaf_0,
            &tree.get_proof_of_leaf(index),
            index as u32,
        )
        .unwrap();
    tree.add_leaf(leaf_0, index);

    let mut last_rmp = merkle_roll.rightmost_proof;

    // Append
    let leaf_1 = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf_1).unwrap();
    tree.add_leaf(leaf_1, tree_size);
    tree_size += 1;

    // Now compare something
    if merkle_roll.get_change_log().get_root() != tree.get_root() {
        let last_active_index: usize =
            (merkle_roll.active_index as usize + BUFFER_SIZE - 1) % BUFFER_SIZE;
        println!("{:?}", &last_rmp);
    }
    last_rmp = merkle_roll.rightmost_proof;
    assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());
}

#[tokio::test(threaded_scheduler)]
/// Append after also appending via a replace
async fn test_append_bug_repro_2() {
    let (mut merkle_roll, mut tree) = setup();
    let mut rng = thread_rng();
    merkle_roll.initialize().unwrap();

    // Fill both trees with random nodes
    let mut tree_size = 10;
    for i in 0..tree_size {
        let leaf = rng.gen::<[u8; 32]>();
        tree.add_leaf(leaf, i);
        merkle_roll.append(leaf).unwrap();
    }
    assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());

    // Replace the rightmost leaf
    let mut leaf = rng.gen::<[u8; 32]>();
    let index = 10;
    merkle_roll
        .set_leaf(
            tree.get_root(),
            tree.get_leaf(index),
            leaf,
            &tree.get_proof_of_leaf(index),
            index as u32,
        )
        .unwrap();
    tree.add_leaf(leaf, index);
    tree_size += 1;

    let mut last_rmp = merkle_roll.rightmost_proof;

    // Append
    leaf = rng.gen::<[u8; 32]>();
    merkle_roll.append(leaf).unwrap();
    tree.add_leaf(leaf, tree_size);
    tree_size += 1;

    // Now compare something
    if merkle_roll.get_change_log().get_root() != tree.get_root() {
        let last_active_index: usize =
            (merkle_roll.active_index as usize + BUFFER_SIZE - 1) % BUFFER_SIZE;
        println!("{:?}", &last_rmp);
    }
    last_rmp = merkle_roll.rightmost_proof;
    assert_eq!(merkle_roll.get_change_log().get_root(), tree.get_root());
}
