extern crate rand;

use self::rand::Rng;

const UNMISTAKABLE_CHARS: &'static str = "23456789ABCDEFGHJKLMNPQRSTWXYZabcdefghijkmnopqrstuvwxyz";

pub struct Random {
    rng: rand::OsRng,
    set: Vec<char>,
}

impl Random {
	pub fn new() -> Self {
		Random {
			rng: rand::OsRng::new().unwrap(),
            set: UNMISTAKABLE_CHARS.chars().collect(),
		}
	}

	pub fn id(&mut self) -> String {
		self.random_string(17)
	}

	pub fn random_string(&mut self, char_count: usize) -> String {
		let mut rand_str = String::with_capacity(char_count);
		for _ in 0..char_count {
            let next = (self.rng.next_f32() * self.set.len() as f32) as usize;
			rand_str.push(self.set.get(next).unwrap().clone());
		}
		rand_str
	}
}
