use std::cell::{Cell, UnsafeCell};
use std::fmt::Debug;
use std::iter;

use cfg::{Callback, CfgParam};
use indexmap::IndexMap;
use stdx::{impl_debug, impl_idx_from};
use typed_indexmap::{TiMap, TiSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unkown(u32);
impl_idx_from!(Unkown(u32));

impl From<FirstOrderUnkown> for Unkown {
    fn from(unkown: FirstOrderUnkown) -> Unkown {
        Unkown(unkown.0)
    }
}

impl_debug! {match Unkown{
    Unkown(raw) => "unkown{}",raw;
}}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FirstOrderUnkown(u32);
impl_idx_from!(FirstOrderUnkown(u32));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NthOrderUnkown(u32);
impl_idx_from!(NthOrderUnkown(u32));

pub type FirstOrderUnkownInfo = Box<[(CfgParam, u64)]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NthOrderUnkownInfo {
    previous_order: Unkown,
    base: FirstOrderUnkown,
}

pub struct Unkowns {
    first_order_unkowns: TiMap<FirstOrderUnkown, Callback, FirstOrderUnkownInfo>,
    higher_order_unkowns: UnsafeCell<TiSet<NthOrderUnkown, NthOrderUnkownInfo>>,
    buf: Cell<Vec<FirstOrderUnkown>>,
}

impl Unkowns {
    pub fn len(&self) -> usize {
        self.first_order_unkowns.len() + unsafe { &*self.higher_order_unkowns.get() }.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn new(unkowns: impl IntoIterator<Item = (Callback, FirstOrderUnkownInfo)>) -> Unkowns {
        let first_order_unkowns: IndexMap<_, _, ahash::RandomState> = unkowns.into_iter().collect();
        Self {
            first_order_unkowns: first_order_unkowns.into(),
            higher_order_unkowns: UnsafeCell::new(TiSet::default()),
            // don't expect more than 8. th order derivative in most code
            buf: Cell::new(Vec::with_capacity(8)),
        }
    }

    // pub fn new(unkowns: TiMap<NthOrderUnkownInfo, Callback, FirstOrderUnkownInfo>) -> Unkowns {
    //     Unkowns { first_order_unkowns: unkowns, higher_order_unkowns: UnsafeCell::new(Vec::new()) }
    // }

    pub fn param_derivative(&self, param: CfgParam, unkown: FirstOrderUnkown) -> f64 {
        self.first_order_unkowns[unkown]
            .iter()
            .find(|(it, _)| *it == param)
            .map_or(0.0, |(_, val)| f64::from_bits(*val))
    }

    pub fn previous_order(&self, unkown: Unkown) -> Option<Unkown> {
        if (unkown.0 as usize) < self.first_order_unkowns.len() {
            None
        } else {
            Some(
                self.nth_order_info((usize::from(unkown) - self.first_order_unkowns.len()).into())
                    .previous_order,
            )
        }
    }

    pub fn to_first_order(&self, unkown: Unkown) -> FirstOrderUnkown {
        if (unkown.0 as usize) < self.first_order_unkowns.len() {
            unkown.0.into()
        } else {
            self.nth_order_info((usize::from(unkown) - self.first_order_unkowns.len()).into()).base
        }
    }

    pub fn first_order_unkowns(
        &self,
        unkown: Unkown,
    ) -> impl Iterator<Item = FirstOrderUnkown> + '_ {
        iter::successors(Some(unkown), |it| self.previous_order(*it))
            .map(|unkown| self.to_first_order(unkown))
    }

    #[allow(clippy::needless_collect)] // false positive can't revese successors
    pub fn first_order_unkowns_rev(
        &self,
        unkown: Unkown,
    ) -> impl Iterator<Item = FirstOrderUnkown> + '_ {
        let unkowns: Vec<_> = iter::successors(Some(unkown), |it| self.previous_order(*it))
            .map(|unkown| self.to_first_order(unkown))
            .collect();
        unkowns.into_iter().rev()
    }

    fn nth_order_info(&self, unkown: NthOrderUnkown) -> NthOrderUnkownInfo {
        // This is save since we never hand out a reference (only a copy)
        let unkowns = unsafe { &*self.higher_order_unkowns.get() };
        unkowns[unkown]
    }

    pub fn raise_order(&self, unkown: Unkown, next_unkown: FirstOrderUnkown) -> Unkown {
        // This is save since we never hand out a reference

        let mut prev_orders = self.buf.take();
        prev_orders.extend(self.first_order_unkowns(unkown));

        let mut curr = next_unkown.into();
        for base in prev_orders.drain(..).rev() {
            let unkown = unsafe { &mut *self.higher_order_unkowns.get() }
                .ensure(NthOrderUnkownInfo { previous_order: curr, base })
                .0;
            curr = Unkown(unkown.0 + self.first_order_unkowns.len() as u32);
        }

        self.buf.set(prev_orders);

        curr
    }

    pub fn callback_unkown(&self, callback: Callback) -> Option<FirstOrderUnkown> {
        self.first_order_unkowns.index(&callback)
    }
}
