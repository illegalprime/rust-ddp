extern crate rand;

use self::rand::Rng;

const UNMISTAKABLE_CHARS: &'static str = "23456789ABCDEFGHJKLMNPQRSTWXYZabcdefghijkmnopqrstuvwxyz";

pub struct Random {
    rng: rand::OsRng,
}

impl Random {
	pub fn new() -> Self {
		Random {
			rng: rand::OsRng::new().unwrap(),
		}
	}

	pub fn id(&mut self) -> String {
		self.random_string(17, UNMISTAKABLE_CHARS)
	}

	pub fn random_string(&mut self, char_count: usize, alphabet: &'static str) -> String {
		let mut rand_str = String::with_capacity(char_count);
		for _ in 0..char_count {
			rand_str.push(self.pick_char(alphabet));
		}
		rand_str
	}

	pub fn pick_char(&mut self, alphabet: &'static str) -> char {
		// TODO: Make faster
		alphabet.chars().nth((self.rng.next_f32() * alphabet.len() as f32) as usize).unwrap()
	}
}
