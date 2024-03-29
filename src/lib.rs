//! The Sinsemilla hash function and commitment scheme.
#![no_std]

use group::Curve;

use pasta_curves::arithmetic::{CurveAffine, CurveExt};
use pasta_curves::pallas;
use subtle::CtOption;

//use crate::spec::{extract_p_bottom, i2lebsp};

mod addition;
use self::addition::IncompletePoint;

mod constants;
//mod sinsemilla_s;
pub use constants::*;
//pub(crate) use sinsemilla_s::*;

fn extract_p(point: &pallas::Point) -> pallas::Base {
    point
        .to_affine()
        .coordinates()
        .map(|c| *c.x())
        .unwrap_or_else(pallas::Base::zero)
}

fn extract_p_bottom(point: CtOption<pallas::Point>) -> CtOption<pallas::Base> {
    point.map(|p| extract_p(&p))
}

/// The sequence of bits representing a u64 in little-endian order.
///
/// # Panics
///
/// Panics if the expected length of the sequence `NUM_BITS` exceeds
/// 64.
fn i2lebsp<const NUM_BITS: usize>(int: u64) -> [bool; NUM_BITS] {
    assert!(NUM_BITS <= 64);
    let mut res = [false; NUM_BITS];
    for i in 0..NUM_BITS {
        res[i] = (int & (1 << i)) != 0
    }
    return res;
}

pub(crate) fn lebs2ip_k(bits: &[bool]) -> u32 {
    assert!(bits.len() == K);
    bits.iter()
        .enumerate()
        .fold(0u32, |acc, (i, b)| acc + if *b { 1 << i } else { 0 })
}

/// The sequence of K bits in little-endian order representing an integer
/// up to `2^K` - 1.
pub(crate) fn i2lebsp_k(int: usize) -> [bool; K] {
    assert!(int < (1 << K));
    i2lebsp(int as u64)
}

/// Pads the given iterator (which MUST have length $\leq K * C$) with zero-bits to a
/// multiple of $K$ bits.
struct Chunks<I: Iterator<Item = bool>> {
    inner: I,
    index: usize,
}

impl<I: Iterator<Item = bool>> Chunks<I> {
    fn new(inner: I) -> Self {
        Chunks { inner, index: 0 }
    }
}

impl<I: Iterator<Item = bool>> Iterator for Chunks<I> {
    type Item = [bool; 10];

    fn next(&mut self) -> Option<Self::Item> {
        let fst = self.inner.next();
        if fst.is_none() {
            return None;
        }
        Some([
            fst.unwrap(),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
            self.inner.next().unwrap_or(false),
        ])
    }
}

/// A domain in which $\mathsf{SinsemillaHashToPoint}$ and $\mathsf{SinsemillaHash}$ can
/// be used.
#[derive(Debug, Clone)]
#[allow(non_snake_case)]
pub struct HashDomain {
    Q: pallas::Point,
}

// TODO: j < 2^K check
fn sinsemilla_s(j: usize) -> (pallas::Base, pallas::Base) {
    let hash = pallas::Point::unboxed_hash_to_curve(S_PERSONALIZATION, &j.to_le_bytes());
    let point = hash.to_affine().coordinates().unwrap();
    (*point.x(), *point.y())
}

impl HashDomain {
    /// Constructs a new `HashDomain` with a specific prefix string.
    pub fn new(domain: &str) -> Self {
        HashDomain {
            Q: pallas::Point::unboxed_hash_to_curve(Q_PERSONALIZATION, domain.as_bytes()),
        }
    }

    /// $\mathsf{SinsemillaHashToPoint}$ from [§ 5.4.1.9][concretesinsemillahash].
    ///
    /// [concretesinsemillahash]: https://zips.z.cash/protocol/nu5.pdf#concretesinsemillahash
    pub fn hash_to_point(&self, msg: impl Iterator<Item = bool>) -> CtOption<pallas::Point> {
        self.hash_to_point_inner(msg).into()
    }

    #[allow(non_snake_case)]
    fn hash_to_point_inner(&self, msg: impl Iterator<Item = bool>) -> IncompletePoint {
        Chunks::new(msg).fold(IncompletePoint::from(self.Q), |acc, chunk| {
            let (S_x, S_y) = sinsemilla_s(lebs2ip_k(&chunk) as usize);
            let S_chunk = pallas::Affine::from_xy(S_x, S_y).unwrap();
            (acc + S_chunk) + acc
        })
    }

    /// $\mathsf{SinsemillaHash}$ from [§ 5.4.1.9][concretesinsemillahash].
    ///
    /// [concretesinsemillahash]: https://zips.z.cash/protocol/nu5.pdf#concretesinsemillahash
    ///
    /// # Panics
    ///
    /// This panics if the message length is greater than [`K`] * [`C`]
    pub fn hash(&self, msg: impl Iterator<Item = bool>) -> CtOption<pallas::Base> {
        extract_p_bottom(self.hash_to_point(msg))
    }

    /// Returns the Sinsemilla $Q$ constant for this domain.
    #[cfg(test)]
    #[allow(non_snake_case)]
    pub(crate) fn Q(&self) -> pallas::Point {
        self.Q
    }
}

/// A domain in which $\mathsf{SinsemillaCommit}$ and $\mathsf{SinsemillaShortCommit}$ can
/// be used.
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct CommitDomain {
    M: HashDomain,
    R: pallas::Point,
}

// TODO: no_std string concat
impl CommitDomain {
    /// Constructs a new `CommitDomain` with a specific prefix string.
    pub fn new(domain: &str) -> Self {
        //let m_prefix = format!("{}-M", domain);
        //let r_prefix = format!("{}-r", domain);
        let mut m_buffer = [0; 64];
        let mut r_buffer = [0; 64];
        let m_prefix = no_alloc_concat(domain, "-M", &mut m_buffer);
        let r_prefix = no_alloc_concat(domain, "-r", &mut r_buffer);

        CommitDomain {
            M: HashDomain::new(&m_prefix),
            R: pallas::Point::unboxed_hash_to_curve(&r_prefix, &[]),
        }
    }

    /// $\mathsf{SinsemillaCommit}$ from [§ 5.4.8.4][concretesinsemillacommit].
    ///
    /// [concretesinsemillacommit]: https://zips.z.cash/protocol/nu5.pdf#concretesinsemillacommit
    #[allow(non_snake_case)]
    pub fn commit(
        &self,
        msg: impl Iterator<Item = bool>,
        r: &pallas::Scalar,
    ) -> CtOption<pallas::Point> {
        // We use complete addition for the blinding factor.
        CtOption::<pallas::Point>::from(self.M.hash_to_point_inner(msg)).map(|p| p + self.R * r)
        // TODO: left multiplication
    }

    /// $\mathsf{SinsemillaShortCommit}$ from [§ 5.4.8.4][concretesinsemillacommit].
    ///
    /// [concretesinsemillacommit]: https://zips.z.cash/protocol/nu5.pdf#concretesinsemillacommit
    pub fn short_commit(
        &self,
        msg: impl Iterator<Item = bool>,
        r: &pallas::Scalar,
    ) -> CtOption<pallas::Base> {
        extract_p_bottom(self.commit(msg, r))
    }

    /// Returns the Sinsemilla $R$ constant for this domain.
    #[cfg(test)]
    #[allow(non_snake_case)]
    pub(crate) fn R(&self) -> pallas::Point {
        self.R
    }
}

fn no_alloc_concat<'a, 'b, 'c>(
    first: &'a str,
    second: &'b str,
    buffer: &'c mut [u8; 64],
) -> &'c str {
    use core::str::from_utf8;
    let len = first.len() + second.len();
    assert!(len <= 64);
    for (buf, x) in (&mut buffer[..]).iter_mut().zip(first.bytes()) {
        *buf = x;
    }
    for (buf, x) in (&mut buffer[second.len()..]).iter_mut().zip(second.bytes()) {
        *buf = x;
    }
    from_utf8(&buffer[..len]).unwrap()
}

/*
#[cfg(all(test, feature = "std"))]
mod tests {
    use super::{i2lebsp_k, lebs2ip_k, Pad, K};
    use rand::{self, rngs::OsRng, Rng};
    use std::vec::{vec, Vec};

    /*#[test]
    fn pad() {
        assert_eq!(Pad::new([].iter().cloned()).collect::<Vec<_>>(), vec![]);
        assert_eq!(
            Pad::new([true].iter().cloned()).collect::<Vec<_>>(),
            vec![true, false, false, false, false, false, false, false, false, false]
        );
        assert_eq!(
            Pad::new([true, true].iter().cloned()).collect::<Vec<_>>(),
            vec![true, true, false, false, false, false, false, false, false, false]
        );
        assert_eq!(
            Pad::new([true, true, true].iter().cloned()).collect::<Vec<_>>(),
            vec![true, true, true, false, false, false, false, false, false, false]
        );
        assert_eq!(
            Pad::new(
                [true, true, false, true, false, true, false, true, false, true]
                    .iter()
                    .cloned()
            )
            .collect::<Vec<_>>(),
            vec![true, true, false, true, false, true, false, true, false, true]
        );
        assert_eq!(
            Pad::new(
                [true, true, false, true, false, true, false, true, false, true, true]
                    .iter()
                    .cloned()
            )
            .collect::<Vec<_>>(),
            vec![
                true, true, false, true, false, true, false, true, false, true, true, false, false,
                false, false, false, false, false, false, false
            ]
        );*/
    }

    #[test]
    fn lebs2ip_k_round_trip() {
        let mut rng = OsRng;
        {
            let int = rng.gen_range(0..(1 << K));
            assert_eq!(lebs2ip_k(&i2lebsp_k(int)) as usize, int);
        }

        assert_eq!(lebs2ip_k(&i2lebsp_k(0)) as usize, 0);
        assert_eq!(lebs2ip_k(&i2lebsp_k((1 << K) - 1)) as usize, (1 << K) - 1);
    }

    #[test]
    fn i2lebsp_k_round_trip() {
        {
            let bitstring = (0..K).map(|_| rand::random()).collect::<Vec<_>>();
            assert_eq!(
                i2lebsp_k(lebs2ip_k(&bitstring) as usize).to_vec(),
                bitstring
            );
        }

        {
            let bitstring = [false; K];
            assert_eq!(
                i2lebsp_k(lebs2ip_k(&bitstring) as usize).to_vec(),
                bitstring
            );
        }

        {
            let bitstring = [true; K];
            assert_eq!(
                i2lebsp_k(lebs2ip_k(&bitstring) as usize).to_vec(),
                bitstring
            );
        }
    }
}*/

#[test]
fn main() {
    //use std;
    //std::println!("========================");
    let domain = HashDomain::new("test");
    let message = [
        true, true, false, true, false, true, false, true, false, true, true,
    ]
    .iter()
    .cloned();
    let p = domain.hash_to_point(message);
    //std::println!("Hello {:?}", p);
    //std::println!("========================");
}
