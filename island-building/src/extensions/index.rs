//! A small owned BVH keeps roof joins local instead of comparing every triangle.
use super::clip::Volume;
use crate::collision::BoxCollider;
use std::ops::Range;

#[derive(Default)]
pub(super) struct Volumes {
    pub items: Vec<Volume>,
    branches: Vec<Branch>,
}

struct Branch {
    bounds: BoxCollider,
    range: Range<usize>,
    children: Option<[usize; 2]>,
}

impl Volumes {
    pub fn build(&mut self) {
        self.branches.clear();
        if !self.items.is_empty() {
            build(&mut self.items, 0, &mut self.branches);
        }
    }
    pub fn query(&self, bounds: BoxCollider) -> Query<'_> {
        let mut query = Query {
            items: &self.items,
            branches: &self.branches,
            stack: [0; 64],
            len: 0,
            current: 0..0,
            bounds,
        };
        if !self.branches.is_empty() {
            query.stack[0] = 0;
            query.len = 1;
        }
        query
    }
}

fn build(items: &mut [Volume], offset: usize, branches: &mut Vec<Branch>) -> usize {
    let bounds = enclosing(items);
    let index = branches.len();
    branches.push(Branch {
        bounds,
        range: offset..offset + items.len(),
        children: None,
    });
    if items.len() > 8 {
        let axis = usize::from(bounds.max[1] - bounds.min[1] > bounds.max[0] - bounds.min[0]);
        items.sort_unstable_by(|a, b| {
            (a.bounds.min[axis] + a.bounds.max[axis])
                .total_cmp(&(b.bounds.min[axis] + b.bounds.max[axis]))
        });
        let middle = items.len() / 2;
        let (left, right) = items.split_at_mut(middle);
        let children = [
            build(left, offset, branches),
            build(right, offset + middle, branches),
        ];
        branches[index].children = Some(children);
    }
    index
}

fn enclosing(items: &[Volume]) -> BoxCollider {
    let mut bounds = items[0].bounds;
    for volume in &items[1..] {
        for axis in 0..3 {
            bounds.min[axis] = bounds.min[axis].min(volume.bounds.min[axis]);
            bounds.max[axis] = bounds.max[axis].max(volume.bounds.max[axis]);
        }
    }
    bounds
}

fn overlaps(a: BoxCollider, b: BoxCollider) -> bool {
    (0..3).all(|i| a.min[i] <= b.max[i] + 0.00001 && a.max[i] >= b.min[i] - 0.00001)
}

pub(super) struct Query<'a> {
    items: &'a [Volume],
    branches: &'a [Branch],
    stack: [usize; 64],
    len: usize,
    current: Range<usize>,
    bounds: BoxCollider,
}

impl<'a> Query<'a> {
    pub fn all(items: &'a [Volume], bounds: BoxCollider) -> Self {
        Self {
            items,
            branches: &[],
            stack: [0; 64],
            len: 0,
            current: 0..items.len(),
            bounds,
        }
    }
}

impl<'a> Iterator for Query<'a> {
    type Item = &'a Volume;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            for index in self.current.by_ref() {
                if overlaps(self.bounds, self.items[index].bounds) {
                    return Some(&self.items[index]);
                }
            }
            if self.len == 0 {
                return None;
            }
            self.len -= 1;
            let branch = &self.branches[self.stack[self.len]];
            if !overlaps(self.bounds, branch.bounds) {
                continue;
            }
            if let Some(children) = branch.children {
                self.stack[self.len] = children[1];
                self.stack[self.len + 1] = children[0];
                self.len += 2;
            } else {
                self.current = branch.range.clone();
            }
        }
    }
}
