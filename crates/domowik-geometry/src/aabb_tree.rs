//! A dynamic bounding-volume hierarchy over axis-aligned boxes, maintained
//! entry by entry.
//!
//! # Why a tree, and why this one
//!
//! [`crate::spatial::SpatialIndex`] is a uniform grid rebuilt from scratch for
//! every document revision. That is the right shape for a snapshot, and the
//! wrong one for an index that lives across edits: a grid sized to its input
//! changes cell size and origin as the input changes, an entry spanning many
//! cells is stored many times, and a huge marquee rectangle walks every cell
//! it covers whether or not anything is there.
//!
//! This tree keeps ONE leaf per entry whatever its size, rejects empty space
//! hierarchically (a query visits only subtrees whose box it overlaps), and
//! changes by a root-ward path per insertion or removal: `O(log n)` nodes in
//! a balanced tree. Issue 0012's approach review (Codex GPT-6-Astra,
//! 2026-10-08) recommended it as the default backend for the per-level query
//! indexes; the grid stays for one-shot snapshot queries.
//!
//! # Algorithm and provenance
//!
//! Independent implementation of the standard dynamic AABB tree described
//! in the public literature:
//!
//! - insertion descends from the root choosing, at each internal node,
//!   between pairing the new leaf with that node and descending into the
//!   child whose enlargement costs least, with the box PERIMETER as cost
//!   (the 2D surface-area heuristic; Goldsmith & Salmon 1987, MacDonald &
//!   Booth 1990);
//! - balance is restored on the way back up by rotations keyed on subtree
//!   height, as in an AVL tree (Adelson-Velsky & Landis 1962). Unlike AVL
//!   insertion, the cost heuristic may pair a new leaf with a TALL subtree,
//!   so a rotation can leave the demoted node unbalanced too; it is
//!   rebalanced recursively, so every node satisfies the AVL condition
//!   after each update;
//! - removal replaces a leaf's parent by its sibling and refits upwards;
//! - a BULK build into an empty tree ([`AabbTree::extend_empty`]) splits the
//!   entries top-down at the median centre along the longer axis of their
//!   centres' extent (the classic top-down median-split BVH construction),
//!   which gives a much tighter tree than inserting one by one in document
//!   order. Every index starts with one, so later single-entry edits refine a
//!   good tree rather than repair a poor one.
//!
//! The Box2D dynamic-tree DOCUMENTATION was consulted for the operation set
//! (insert / remove / move proxy / query); no source code of any engine was
//! read or adapted.
//!
//! # Contract
//!
//! - Boxes are plan millimetres, as [`Aabb`] everywhere. A box must be
//!   finite with `min <= max`; [`AabbTree::insert`] panics otherwise, because
//!   a NaN bound would make every comparison below meaningless.
//! - [`AabbTree::query`] visits EXACTLY the entries whose stored box shares
//!   a point with the query box (boundary inclusive, [`Aabb::intersects`]).
//!   It is a broad phase over the stored boxes: a caller wanting an exact
//!   answer about the shape inside a box must test that shape itself.
//! - Visit order is an implementation detail and changes with the edit
//!   history. Callers that rank results must sort them by their own key.
//! - [`Proxy`] handles are generational: a handle of a removed entry is
//!   rejected (panic) rather than silently naming whatever reused its slot.
//!
//! # Debugging
//!
//! [`AabbTree::validate`] checks every structural invariant (parent links,
//! heights, exact box unions, balance, leaf count) and panics naming the
//! first one broken.

use crate::spatial::Aabb;

const NONE: u32 = u32::MAX;

/// A stable handle to one entry of an [`AabbTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Proxy {
    index: u32,
    generation: u32,
}

#[derive(Debug, Clone)]
enum Kind<T> {
    Leaf(T),
    Internal { left: u32, right: u32 },
    Free { next: u32 },
}

#[derive(Debug, Clone)]
struct Node<T> {
    aabb: Aabb,
    parent: u32,
    /// Leaves are height 0.
    height: u32,
    generation: u32,
    kind: Kind<T>,
}

/// A dynamic AABB tree of `T`. See the module documentation.
#[derive(Debug, Clone)]
pub struct AabbTree<T> {
    nodes: Vec<Node<T>>,
    root: u32,
    free: u32,
    leaves: usize,
}

impl<T> Default for AabbTree<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn perimeter(b: &Aabb) -> f64 {
    2.0 * ((b.max_x - b.min_x) + (b.max_y - b.min_y))
}

fn valid(b: &Aabb) -> bool {
    [b.min_x, b.min_y, b.max_x, b.max_y]
        .iter()
        .all(|v| v.is_finite())
        && b.min_x <= b.max_x
        && b.min_y <= b.max_y
}

impl<T> AabbTree<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: NONE,
            free: NONE,
            leaves: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.leaves
    }

    pub fn is_empty(&self) -> bool {
        self.leaves == 0
    }

    /// Height of the tree: 0 for one leaf, and for an empty tree.
    pub fn height(&self) -> u32 {
        if self.root == NONE {
            0
        } else {
            self.nodes[self.root as usize].height
        }
    }

    fn allocate(&mut self, aabb: Aabb, kind: Kind<T>) -> u32 {
        if self.free != NONE {
            let i = self.free;
            let node = &mut self.nodes[i as usize];
            let Kind::Free { next } = node.kind else {
                unreachable!("aabb tree: free list names a live node");
            };
            self.free = next;
            node.aabb = aabb;
            node.parent = NONE;
            node.height = 0;
            node.kind = kind;
            i
        } else {
            self.nodes.push(Node {
                aabb,
                parent: NONE,
                height: 0,
                generation: 0,
                kind,
            });
            (self.nodes.len() - 1) as u32
        }
    }

    fn release(&mut self, i: u32) -> Kind<T> {
        let node = &mut self.nodes[i as usize];
        node.generation = node.generation.wrapping_add(1);
        let kind = std::mem::replace(&mut node.kind, Kind::Free { next: self.free });
        self.free = i;
        kind
    }

    fn children(&self, i: u32) -> (u32, u32) {
        match self.nodes[i as usize].kind {
            Kind::Internal { left, right } => (left, right),
            _ => unreachable!("aabb tree: node {i} is not internal"),
        }
    }

    fn leaf(&self, proxy: Proxy) -> &Node<T> {
        let node = self
            .nodes
            .get(proxy.index as usize)
            .expect("aabb tree: proxy out of range");
        assert!(
            node.generation == proxy.generation && matches!(node.kind, Kind::Leaf(_)),
            "aabb tree: stale proxy {proxy:?}"
        );
        node
    }

    /// The entry's stored value.
    pub fn get(&self, proxy: Proxy) -> &T {
        match &self.leaf(proxy).kind {
            Kind::Leaf(v) => v,
            _ => unreachable!(),
        }
    }

    /// The entry's stored box.
    pub fn bounds(&self, proxy: Proxy) -> Aabb {
        self.leaf(proxy).aabb
    }

    /// Adds an entry. Panics on a non-finite or inverted box.
    pub fn insert(&mut self, aabb: Aabb, value: T) -> Proxy {
        assert!(valid(&aabb), "aabb tree: invalid box {aabb:?}");
        let leaf = self.allocate(aabb, Kind::Leaf(value));
        self.insert_leaf(leaf);
        self.leaves += 1;
        Proxy {
            index: leaf,
            generation: self.nodes[leaf as usize].generation,
        }
    }

    /// Removes an entry and returns its value. Panics on a stale proxy.
    pub fn remove(&mut self, proxy: Proxy) -> T {
        self.leaf(proxy);
        self.remove_leaf(proxy.index);
        self.leaves -= 1;
        match self.release(proxy.index) {
            Kind::Leaf(v) => v,
            _ => unreachable!(),
        }
    }

    /// Fills an EMPTY tree with `items` by top-down median split, returning
    /// their proxies in input order. Panics if the tree is not empty or a
    /// box is invalid.
    pub fn extend_empty(&mut self, items: Vec<(Aabb, T)>) -> Vec<Proxy> {
        assert!(
            self.root == NONE,
            "aabb tree: bulk build into a non-empty tree"
        );
        let mut leaves: Vec<u32> = Vec::with_capacity(items.len());
        for (aabb, value) in items {
            assert!(valid(&aabb), "aabb tree: invalid box {aabb:?}");
            leaves.push(self.allocate(aabb, Kind::Leaf(value)));
        }
        self.leaves = leaves.len();
        let proxies = leaves
            .iter()
            .map(|&i| Proxy {
                index: i,
                generation: self.nodes[i as usize].generation,
            })
            .collect();
        if !leaves.is_empty() {
            let mut order = leaves;
            self.root = self.split(&mut order);
            self.nodes[self.root as usize].parent = NONE;
        }
        proxies
    }

    /// Builds a subtree over `leaves`, reordering the slice.
    fn split(&mut self, leaves: &mut [u32]) -> u32 {
        if leaves.len() == 1 {
            return leaves[0];
        }
        let centre = |n: &Node<T>| {
            (
                n.aabb.min_x * 0.5 + n.aabb.max_x * 0.5,
                n.aabb.min_y * 0.5 + n.aabb.max_y * 0.5,
            )
        };
        let (mut lo, mut hi) = (
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for &i in leaves.iter() {
            let c = centre(&self.nodes[i as usize]);
            lo = (lo.0.min(c.0), lo.1.min(c.1));
            hi = (hi.0.max(c.0), hi.1.max(c.1));
        }
        let along_x = hi.0 - lo.0 >= hi.1 - lo.1;
        let mid = leaves.len() / 2;
        let nodes = &self.nodes;
        leaves.select_nth_unstable_by(mid, |&a, &b| {
            let (ca, cb) = (centre(&nodes[a as usize]), centre(&nodes[b as usize]));
            let (ka, kb) = if along_x { (ca.0, cb.0) } else { (ca.1, cb.1) };
            ka.total_cmp(&kb).then(a.cmp(&b))
        });
        let (left, right) = leaves.split_at_mut(mid);
        let (l, r) = (self.split(left), self.split(right));
        let parent = self.allocate(
            self.nodes[l as usize].aabb,
            Kind::Internal { left: l, right: r },
        );
        self.nodes[l as usize].parent = parent;
        self.nodes[r as usize].parent = parent;
        self.refit(parent);
        parent
    }

    /// Changes an entry's box, keeping its value and its handle.
    pub fn update(&mut self, proxy: Proxy, aabb: Aabb) {
        assert!(valid(&aabb), "aabb tree: invalid box {aabb:?}");
        if self.leaf(proxy).aabb == aabb {
            return;
        }
        self.remove_leaf(proxy.index);
        self.nodes[proxy.index as usize].aabb = aabb;
        self.insert_leaf(proxy.index);
    }

    /// Calls `visit` with the proxy, stored box and value of every entry
    /// whose box shares a point with `region` (boundary inclusive), once
    /// each. Allocation-free for trees up to a height of 63.
    pub fn query<'a>(&'a self, region: &Aabb, mut visit: impl FnMut(Proxy, &'a Aabb, &'a T)) {
        if self.root == NONE {
            return;
        }
        let mut inline = [NONE; 64];
        let mut spill: Vec<u32> = Vec::new();
        let mut top = 0usize;
        inline[0] = self.root;
        top += 1;
        loop {
            let i = if let Some(i) = spill.pop() {
                i
            } else if top > 0 {
                top -= 1;
                inline[top]
            } else {
                break;
            };
            let node = &self.nodes[i as usize];
            if !node.aabb.intersects(region) {
                continue;
            }
            match &node.kind {
                Kind::Leaf(v) => visit(
                    Proxy {
                        index: i,
                        generation: node.generation,
                    },
                    &node.aabb,
                    v,
                ),
                Kind::Internal { left, right } => {
                    for c in [*left, *right] {
                        if top < inline.len() {
                            inline[top] = c;
                            top += 1;
                        } else {
                            spill.push(c);
                        }
                    }
                }
                Kind::Free { .. } => unreachable!("aabb tree: free node reachable"),
            }
        }
    }

    /// Every live entry, in storage order.
    pub fn iter(&self) -> impl Iterator<Item = (Proxy, &T)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match &n.kind {
                Kind::Leaf(v) => Some((
                    Proxy {
                        index: i as u32,
                        generation: n.generation,
                    },
                    v,
                )),
                _ => None,
            })
    }

    // --- structure ---------------------------------------------------------

    fn insert_leaf(&mut self, leaf: u32) {
        if self.root == NONE {
            self.root = leaf;
            self.nodes[leaf as usize].parent = NONE;
            return;
        }
        let aabb = self.nodes[leaf as usize].aabb;

        // Descend to the cheapest sibling.
        let mut at = self.root;
        while let Kind::Internal { left, right } = self.nodes[at as usize].kind {
            let here = self.nodes[at as usize].aabb;
            let combined = perimeter(&here.union(aabb));
            // Pairing with `at` creates a parent of this perimeter...
            let pair_cost = 2.0 * combined;
            // ...and descending enlarges `at` by at least this much.
            let inherited = 2.0 * (combined - perimeter(&here));
            let descend = |c: u32| {
                let node = &self.nodes[c as usize];
                let grown = perimeter(&node.aabb.union(aabb));
                match node.kind {
                    Kind::Leaf(_) => grown + inherited,
                    _ => grown - perimeter(&node.aabb) + inherited,
                }
            };
            let (cl, cr) = (descend(left), descend(right));
            if pair_cost < cl && pair_cost < cr {
                break;
            }
            at = if cl <= cr { left } else { right };
        }

        // Splice a new parent above the sibling.
        let sibling = at;
        let old_parent = self.nodes[sibling as usize].parent;
        let parent = self.allocate(
            self.nodes[sibling as usize].aabb.union(aabb),
            Kind::Internal {
                left: sibling,
                right: leaf,
            },
        );
        self.nodes[parent as usize].parent = old_parent;
        self.nodes[sibling as usize].parent = parent;
        self.nodes[leaf as usize].parent = parent;
        if old_parent == NONE {
            self.root = parent;
        } else {
            self.replace_child(old_parent, sibling, parent);
        }
        self.refit_from(parent);
    }

    fn remove_leaf(&mut self, leaf: u32) {
        if leaf == self.root {
            self.root = NONE;
            return;
        }
        let parent = self.nodes[leaf as usize].parent;
        let grand = self.nodes[parent as usize].parent;
        let (l, r) = self.children(parent);
        let sibling = if l == leaf { r } else { l };
        if grand == NONE {
            self.root = sibling;
            self.nodes[sibling as usize].parent = NONE;
        } else {
            self.replace_child(grand, parent, sibling);
            self.nodes[sibling as usize].parent = grand;
        }
        self.release(parent);
        self.nodes[leaf as usize].parent = NONE;
        if grand != NONE {
            self.refit_from(grand);
        }
    }

    fn replace_child(&mut self, parent: u32, old: u32, new: u32) {
        let Kind::Internal { left, right } = &mut self.nodes[parent as usize].kind else {
            unreachable!("aabb tree: parent is not internal");
        };
        if *left == old {
            *left = new;
        } else {
            debug_assert_eq!(*right, old);
            *right = new;
        }
    }

    /// Rebalances and refits every node from `i` to the root.
    fn refit_from(&mut self, mut i: u32) {
        while i != NONE {
            i = self.balance(i);
            self.refit(i);
            i = self.nodes[i as usize].parent;
        }
    }

    fn refit(&mut self, i: u32) {
        let (l, r) = self.children(i);
        let (a, b) = (&self.nodes[l as usize], &self.nodes[r as usize]);
        let (aabb, height) = (a.aabb.union(b.aabb), 1 + a.height.max(b.height));
        let node = &mut self.nodes[i as usize];
        node.aabb = aabb;
        node.height = height;
    }

    /// Restores the AVL condition in the subtree rooted at `a`, whose
    /// children's subtrees are each balanced, and returns its new root.
    ///
    /// Rotates the taller child up while the children differ by more than
    /// one. A rotation demotes `a` beside the lower grandchild, which can be
    /// far shorter than its new sibling when insertion paired a leaf with a
    /// tall subtree (Codex GPT-6.1-Sol review: 0 vs 5), so the demoted node
    /// is rebalanced recursively before the new root is checked again.
    ///
    /// ```text
    ///       a                 c
    ///      / \               / \
    ///     b   c     =>      a   g      (g the taller of c's children)
    ///        / \           / \
    ///       f   g         b   f
    /// ```
    fn balance(&mut self, mut a: u32) -> u32 {
        loop {
            let (b, c) = self.children(a);
            let (hb, hc) = (self.nodes[b as usize].height, self.nodes[c as usize].height);
            let top = if hc > hb + 1 {
                self.rotate_up(a, c, b)
            } else if hb > hc + 1 {
                self.rotate_up(a, b, c)
            } else {
                return a;
            };
            // `a` is now a child of `top`: repair it, then re-check `top`.
            self.balance(a);
            self.refit(top);
            a = top;
        }
    }

    /// Rotates `tall` (a child of `a`; `short` is the other) into `a`'s place.
    fn rotate_up(&mut self, a: u32, tall: u32, short: u32) -> u32 {
        let (f, g) = self.children(tall);
        let (f, g) = if self.nodes[f as usize].height > self.nodes[g as usize].height {
            (g, f)
        } else {
            (f, g)
        };
        // `tall` takes `a`'s place.
        let up = self.nodes[a as usize].parent;
        self.nodes[tall as usize].parent = up;
        if up == NONE {
            self.root = tall;
        } else {
            self.replace_child(up, a, tall);
        }
        // `a` keeps `short` and takes the lower of `tall`'s children; `tall`
        // keeps the higher one.
        self.nodes[a as usize].kind = Kind::Internal {
            left: short,
            right: f,
        };
        self.nodes[f as usize].parent = a;
        self.nodes[a as usize].parent = tall;
        self.nodes[tall as usize].kind = Kind::Internal { left: a, right: g };
        self.refit(a);
        self.refit(tall);
        tall
    }

    /// Checks every structural invariant; panics naming the first broken.
    /// Proportional to the tree size — for tests and debugging.
    pub fn validate(&self) {
        let mut leaves = 0usize;
        if self.root != NONE {
            assert_eq!(
                self.nodes[self.root as usize].parent, NONE,
                "aabb tree: root has a parent"
            );
            let mut stack = vec![self.root];
            while let Some(i) = stack.pop() {
                let node = &self.nodes[i as usize];
                match node.kind {
                    Kind::Leaf(_) => {
                        assert_eq!(node.height, 0, "aabb tree: leaf {i} has height");
                        leaves += 1;
                    }
                    Kind::Internal { left, right } => {
                        let (a, b) = (&self.nodes[left as usize], &self.nodes[right as usize]);
                        assert_eq!(a.parent, i, "aabb tree: child {left} parent link");
                        assert_eq!(b.parent, i, "aabb tree: child {right} parent link");
                        assert_eq!(
                            node.height,
                            1 + a.height.max(b.height),
                            "aabb tree: node {i} height"
                        );
                        assert!(
                            a.height.abs_diff(b.height) <= 1,
                            "aabb tree: node {i} unbalanced ({} vs {})",
                            a.height,
                            b.height
                        );
                        assert_eq!(
                            node.aabb,
                            a.aabb.union(b.aabb),
                            "aabb tree: node {i} box is not its children's union"
                        );
                        stack.push(left);
                        stack.push(right);
                    }
                    Kind::Free { .. } => panic!("aabb tree: free node {i} is reachable"),
                }
            }
        }
        assert_eq!(leaves, self.leaves, "aabb tree: leaf count");
        let mut free = 0usize;
        let mut f = self.free;
        while f != NONE {
            let Kind::Free { next } = self.nodes[f as usize].kind else {
                panic!("aabb tree: free list names live node {f}");
            };
            free += 1;
            f = next;
        }
        let internal = leaves.saturating_sub(1);
        assert_eq!(
            free + leaves + internal,
            self.nodes.len(),
            "aabb tree: nodes leaked"
        );
    }
}

/// An [`AabbTree`] whose entries are owned by keys and kept equal to a
/// desired set by diffing.
///
/// Each key carries a STAMP — the inputs its entry is derived from. A sync
/// walks the complete desired `(key, stamp)` set once: an unchanged stamp
/// keeps its entry untouched, a new or changed one rebuilds it via `make`,
/// and keys not seen are removed. Change detection is therefore exactly as
/// precise as the stamp, and costs one stamp comparison per key; only
/// changed keys pay for geometry or tree work.
///
/// `make` may return `None` (no geometry, e.g. a degenerate wall); the key
/// is still remembered with its stamp, so it is not rebuilt every sync.
#[derive(Debug, Clone)]
pub struct KeyedTree<K, S, V> {
    tree: AabbTree<V>,
    entries: crate::overlay::FastMap<K, (Option<Proxy>, S, u64)>,
    epoch: u64,
}

impl<K, S, V> Default for KeyedTree<K, S, V> {
    fn default() -> Self {
        Self {
            tree: AabbTree::new(),
            entries: Default::default(),
            epoch: 0,
        }
    }
}

impl<K: std::hash::Hash + Eq + Clone, S: PartialEq, V> KeyedTree<K, S, V> {
    /// Brings the tree to `want`, which must name every desired key once.
    /// Returns how many entries were added, rebuilt or removed.
    pub fn sync(
        &mut self,
        want: impl IntoIterator<Item = (K, S)>,
        mut make: impl FnMut(&K, &S) -> Option<(Aabb, V)>,
    ) -> usize {
        self.epoch += 1;
        let epoch = self.epoch;
        if self.entries.is_empty() {
            return self.fill(want, make, epoch);
        }
        let mut changed = 0;
        for (key, stamp) in want {
            if let Some((_, old, mark)) = self.entries.get_mut(&key) {
                if *old == stamp {
                    *mark = epoch;
                    continue;
                }
            }
            if let Some((Some(proxy), _, _)) = self.entries.remove(&key) {
                self.tree.remove(proxy);
            }
            let proxy = make(&key, &stamp).map(|(b, v)| self.tree.insert(b, v));
            self.entries.insert(key, (proxy, stamp, epoch));
            changed += 1;
        }
        let tree = &mut self.tree;
        self.entries.retain(|_, (proxy, _, mark)| {
            let keep = *mark == epoch;
            if !keep {
                if let Some(p) = proxy {
                    tree.remove(*p);
                }
                changed += 1;
            }
            keep
        });
        changed
    }

    /// First sync: every key is new, so the tree is bulk-built.
    fn fill(
        &mut self,
        want: impl IntoIterator<Item = (K, S)>,
        mut make: impl FnMut(&K, &S) -> Option<(Aabb, V)>,
        epoch: u64,
    ) -> usize {
        let mut items = Vec::new();
        let mut keyed = Vec::new();
        for (key, stamp) in want {
            match make(&key, &stamp) {
                Some(item) => {
                    items.push(item);
                    keyed.push((key, stamp, true));
                }
                None => keyed.push((key, stamp, false)),
            }
        }
        let mut proxies = self.tree.extend_empty(items).into_iter();
        let n = keyed.len();
        for (key, stamp, has) in keyed {
            let proxy = if has { proxies.next() } else { None };
            self.entries.insert(key, (proxy, stamp, epoch));
        }
        n
    }

    /// The tree, for queries.
    pub fn tree(&self) -> &AabbTree<V> {
        &self.tree
    }

    /// Number of keys (with or without geometry).
    pub fn keys(&self) -> usize {
        self.entries.len()
    }

    /// Structural check plus key/leaf agreement; panics on violation.
    pub fn validate(&self) {
        self.tree.validate();
        let leaves = self.entries.values().filter(|e| e.0.is_some()).count();
        assert_eq!(
            leaves,
            self.tree.len(),
            "keyed tree: leaves and keys disagree"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::Point;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() * n as f64) as usize % n.max(1)
        }

        fn aabb(&mut self) -> Aabb {
            let (x, y) = (self.next() * 10_000.0, self.next() * 10_000.0);
            // Mostly wall-sized boxes, sometimes a huge one or a point.
            let (w, h) = match self.below(10) {
                0 => (self.next() * 20_000.0, self.next() * 200.0),
                1 => (0.0, 0.0),
                _ => (self.next() * 3000.0, self.next() * 300.0),
            };
            Aabb::from_corners(Point::new(x, y), Point::new(x + w, y + h))
        }
    }

    fn naive(entries: &[(Proxy, Aabb, u32)], region: &Aabb) -> Vec<u32> {
        let mut v: Vec<u32> = entries
            .iter()
            .filter(|(_, b, _)| b.intersects(region))
            .map(|(_, _, t)| *t)
            .collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn random_insert_remove_update_streams_agree_with_a_naive_scan() {
        for seed in 1..=8u64 {
            let mut rng = Rng(0x2545_f491_4f6c_dd1d ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let mut tree = AabbTree::new();
            let mut live: Vec<(Proxy, Aabb, u32)> = Vec::new();
            for step in 0..600u32 {
                match rng.below(10) {
                    0..=4 => {
                        let b = rng.aabb();
                        live.push((tree.insert(b, step), b, step));
                    }
                    5..=6 if !live.is_empty() => {
                        let (p, _, t) = live.swap_remove(rng.below(live.len()));
                        assert_eq!(tree.remove(p), t);
                    }
                    7..=8 if !live.is_empty() => {
                        let i = rng.below(live.len());
                        let b = rng.aabb();
                        tree.update(live[i].0, b);
                        live[i].1 = b;
                    }
                    _ => {}
                }
                tree.validate();
                let q = rng.aabb();
                let mut got = Vec::new();
                tree.query(&q, |_, _, t| got.push(*t));
                got.sort_unstable();
                assert_eq!(got, naive(&live, &q), "seed {seed} step {step}");
            }
            // Height stays logarithmic: AVL bound 1.44·log2(n + 2).
            let n = tree.len() as f64;
            assert!(
                (tree.height() as f64) <= 1.45 * (n + 2.0).log2() + 1.0,
                "height {} for {n} leaves",
                tree.height()
            );
        }
    }

    #[test]
    fn a_keyed_tree_rebuilds_only_changed_stamps_and_drops_unseen_keys() {
        let mut keyed: KeyedTree<u32, u32, u32> = KeyedTree::default();
        let made = std::cell::Cell::new(0);
        let sync = |keyed: &mut KeyedTree<u32, u32, u32>, want: &[(u32, u32)]| {
            keyed.sync(want.iter().copied(), |&k, &s| {
                made.set(made.get() + 1);
                (s != 0).then(|| (Aabb::point(Point::new(s as f64, 0.0)), k))
            })
        };
        assert_eq!(sync(&mut keyed, &[(1, 10), (2, 20), (3, 0)]), 3);
        assert_eq!(sync(&mut keyed, &[(1, 10), (2, 21), (3, 0)]), 1);
        assert_eq!(sync(&mut keyed, &[(2, 21)]), 2);
        keyed.validate();
        assert_eq!(keyed.keys(), 1);
        assert_eq!(keyed.tree().len(), 1);
        assert_eq!(made.get(), 4);
    }

    #[test]
    fn a_bulk_built_tree_is_balanced_and_then_edits_like_any_other() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let mut tree = AabbTree::new();
        let boxes: Vec<Aabb> = (0..500).map(|_| rng.aabb()).collect();
        let proxies = tree.extend_empty(
            boxes
                .iter()
                .enumerate()
                .map(|(i, b)| (*b, i as u32))
                .collect(),
        );
        tree.validate();
        let mut live: Vec<(Proxy, Aabb, u32)> = proxies
            .into_iter()
            .zip(&boxes)
            .enumerate()
            .map(|(i, (p, b))| (p, *b, i as u32))
            .collect();
        for step in 0..200u32 {
            let i = rng.below(live.len());
            let b = rng.aabb();
            tree.update(live[i].0, b);
            live[i].1 = b;
            if step % 3 == 0 {
                let (p, _, _) = live.swap_remove(rng.below(live.len()));
                tree.remove(p);
            }
            tree.validate();
            let q = rng.aabb();
            let mut got = Vec::new();
            tree.query(&q, |_, _, t| got.push(*t));
            got.sort_unstable();
            assert_eq!(got, naive(&live, &q), "step {step}");
        }
    }

    /// Codex GPT-6.1-Sol review: an enclosing box inserted over a bulk-built
    /// tree pairs a leaf with the whole tree; one rotation left the demoted
    /// node unbalanced 0 vs 5.
    #[test]
    fn an_enclosing_insert_over_a_tall_tree_stays_balanced() {
        let mut tree = AabbTree::new();
        tree.extend_empty(
            (0..64)
                .map(|i| (Aabb::point(Point::new(i as f64, 0.0)), i))
                .collect(),
        );
        tree.validate();
        let huge = Aabb::from_corners(Point::new(-1e4, -1e4), Point::new(1e4, 1e4));
        for i in 0..5 {
            tree.insert(huge, 100 + i);
            tree.validate();
        }
    }

    #[test]
    fn a_stale_proxy_is_rejected_even_after_its_slot_is_reused() {
        let mut tree = AabbTree::new();
        let b = Aabb::from_corners(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
        let p = tree.insert(b, 1);
        tree.remove(p);
        let q = tree.insert(b, 2);
        assert_eq!(*tree.get(q), 2);
        let stale = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| *tree.get(p)));
        assert!(stale.is_err());
    }

    #[test]
    fn boundary_contact_is_a_hit_and_a_huge_query_stays_exact() {
        let mut tree = AabbTree::new();
        let unit = |x: f64| Aabb::from_corners(Point::new(x, 0.0), Point::new(x + 1.0, 1.0));
        for i in 0..100 {
            tree.insert(unit(i as f64 * 10.0), i);
        }
        let mut hits = Vec::new();
        tree.query(&Aabb::point(Point::new(1.0, 1.0)), |_, _, t| hits.push(*t));
        assert_eq!(hits, vec![0]);
        let mut all = 0;
        tree.query(
            &Aabb::from_corners(Point::new(-1e300, -1e300), Point::new(1e300, 1e300)),
            |_, _, _| all += 1,
        );
        assert_eq!(all, 100);
    }
}
