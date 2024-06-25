use core::fmt;
use std::fmt::Write;

use crate::state::Square;

const fn rank_pattern(ranks: u8) -> u64 {
	0x0101010101010101 * (ranks as u64)
}
fn first_bit(x: u64) -> Option<u8> {
	let idx = x.trailing_zeros() as u8;
	if idx == 64 { None } else { Some(idx) }
}
fn last_bit(x: u64) -> Option<u8> {
	let idx = x.leading_zeros() as u8;
	if idx == 64 { None } else { Some(63 - idx) }
}

#[derive(Clone, Copy, Default)]
pub struct Bb(pub u64);
impl Bb {
	pub const EMPTY: Bb = Bb(0);
	pub fn one(squ: Square) -> Bb {
		Bb(1 << squ.idx)
	}
	pub fn at(self, squ: Square) -> bool {
		(self.0 >> squ.idx) & 1 == 1
	}
	pub fn rank(rank: u8) -> Bb {
		Bb(rank_pattern(1u8 << rank))
	}
	pub fn file(file: u8) -> Bb {
		debug_assert!(file < 8);
		Bb(0x00000000000000ff).shift_right(file)
	}
	pub fn none(self) -> bool {
		self.0 == 0
	}
	pub fn count(self) -> u32 {
		self.0.count_ones()
	}
}

impl fmt::Display for Bb {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		writeln!(f, "Bb(")?;
		for rank in (0..8).rev() {
			f.write_char('|')?;
			for file in 0..8 {
				f.write_char(if self.at(Square::at(file, rank)) { '@' } else { ' ' })?;
			}
			f.write_str("|\n")?;
		}
		write!(f, ")")
	}
}

impl std::ops::BitOr for Bb {
	type Output = Bb;
	fn bitor(self, rhs: Self) -> Bb {
		Bb(self.0 | rhs.0)
	}
}
impl std::ops::BitAnd for Bb {
	type Output = Bb;
	fn bitand(self, rhs: Self) -> Bb {
		Bb(self.0 & rhs.0)
	}
}
impl std::ops::Not for Bb {
	type Output = Bb;
	fn not(self) -> Self::Output {
		Bb(!self.0)
	}
}
impl std::ops::BitOrAssign for Bb {
	fn bitor_assign(&mut self, rhs: Self) {
		self.0 |= rhs.0;
	}
}
impl std::ops::BitAndAssign for Bb {
	fn bitand_assign(&mut self, rhs: Self) {
		self.0 &= rhs.0;
	}
}

pub struct BbIter(u64);
impl std::iter::Iterator for BbIter {
	type Item = Square;
	fn next(&mut self) -> Option<Square> {
		let idx = self.0.trailing_zeros() as u8;
		if idx == 64 {
			None
		} else {
			self.0 ^= 1 << idx;
			Some(Square { idx })
		}
	}
}
impl Bb {
	pub fn iter(self) -> BbIter {
		BbIter(self.0)
	}
}

impl Bb {
	pub const fn shift_up(self, ranks: u8) -> Bb {
		debug_assert!(ranks < 8);
		Bb((self.0 << ranks) & rank_pattern(0xffu8 << ranks))
	}
	pub const fn shift_down(self, ranks: u8) -> Bb {
		debug_assert!(ranks < 8);
		Bb((self.0 >> ranks) & rank_pattern(0xffu8 >> ranks))
	}
	pub const fn shift_ver(self, ranks: i8) -> Bb {
		debug_assert!(ranks > -8 && ranks < 8);
		if ranks >= 0 {
			self.shift_up(ranks as u8)
		} else {
			self.shift_down(-ranks as u8)
		}
	}
	pub const fn shift_left(self, files: u8) -> Bb {
		Bb(self.0 >> (files * 8))
	}
	pub const fn shift_right(self, files: u8) -> Bb {
		Bb(self.0 << (files * 8))
	}
	pub const fn shift_hor(self, files: i8) -> Bb {
		debug_assert!(files > -8 && files < 8);
		if files >= 0 {
			self.shift_right(files as u8)
		} else {
			self.shift_left(-files as u8)
		}
	}
}

pub const KNIGHT_PATTERNS: [Bb; 64] = {
	let mut res = [Bb::EMPTY; 64];
	let mut idx = 0u8;
	while idx < 64 {
		let squ = Square { idx };
		let bb = Bb(0x0a1100110a); // moves from c3
		res[idx as usize] = bb.shift_hor(squ.file() as i8 - 2).shift_ver(squ.rank() as i8 - 2);
		idx += 1;
	}
	res
};
pub const KING_PATTERNS: [Bb; 64] = {
	let mut res = [Bb::EMPTY; 64];
	let mut idx = 0u8;
	while idx < 64 {
		let squ = Square { idx };
		let bb = Bb(0x070507); // moves from b2
		res[idx as usize] = bb.shift_hor(squ.file() as i8 - 1).shift_ver(squ.rank() as i8 - 1);
		idx += 1;
	}
	res
};


const DIAGONALS: [Bb; 15] = {
	let mut res = [Bb::EMPTY; 15];
	let mut idx = 0u8;
	while idx < 15 {
		let bb = Bb(0x8040201008040201); // moves from main diagonal
		res[idx as usize] = bb.shift_hor(idx as i8 - 7);
		idx += 1;
	}
	res
};
const ANTIDIAGONALS: [Bb; 15] = {
	let mut res = [Bb::EMPTY; 15];
	let mut idx = 0u8;
	while idx < 15 {
		let bb = Bb(0x0102040810204080); // moves from main antidiagonal
		res[idx as usize] = bb.shift_hor(idx as i8 - 7);
		idx += 1;
	}
	res
};

pub fn cast_ray(from: Square, pattern: Bb, pieces: Bb) -> Bb {
	let obstacles = pattern & pieces & !Bb::one(from);
	let before = 0xffffffffffffffff >> (63 - from.idx);
	let after = 0xffffffffffffffff << from.idx;
	let obstacle1 = last_bit(obstacles.0 & before).unwrap_or(0);
	let obstacle2 = first_bit(obstacles.0 & after).unwrap_or(63);
	let ones = 1 + obstacle2 - obstacle1;
	let mask = Bb(0xffffffffffffffff >> (64 - ones) << obstacle1);
	let res = pattern & mask;
	res
}
pub fn cast_diagonals(from: Square, pieces: Bb) -> Bb {
	let diag = cast_ray(from, DIAGONALS[(7 + from.file() - from.rank()) as usize], pieces);
	let antidiag = cast_ray(from, ANTIDIAGONALS[(from.file() + from.rank()) as usize], pieces);
	diag | antidiag
}
pub fn cast_cardinals(from: Square, pieces: Bb) -> Bb {
	let hor = cast_ray(from, Bb::rank(from.rank()), pieces);
	let ver = cast_ray(from, Bb::file(from.file()), pieces);
	hor | ver
}