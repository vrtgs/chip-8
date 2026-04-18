use bytemuck::Zeroable;


pub type Seed = [u32; 4];

type State = Seed;


#[derive(Zeroable, Clone)]
pub(crate) struct SimpleRng(State);

pub trait Seeder {
    fn seed(self, seed: &mut Seed);
}

impl<R: rand_core::Rng> Seeder for R {
    fn seed(mut self, seed: &mut Seed) {
        self.fill_bytes(bytemuck::bytes_of_mut::<Seed>(seed))
    }
}

impl SimpleRng {
    pub fn reseed(&mut self, seeder: impl Seeder) {
        seeder.seed(&mut self.0)
    }

    pub fn next_u32(&mut self) -> u32 {
        // taken from https://xoshiro.di.unimi.it/xoshiro128plusplus.c

        let res = self.0[0]
            .wrapping_add(self.0[3])
            .rotate_left(7)
            .wrapping_add(self.0[0]);

        let t = self.0[1] << 9;

        self.0[2] ^= self.0[0];
        self.0[3] ^= self.0[1];
        self.0[1] ^= self.0[2];
        self.0[0] ^= self.0[3];

        self.0[2] ^= t;

        self.0[3] = self.0[3].rotate_left(11);

        res
    }

    pub fn next_u8(&mut self) -> u8 {
        self.next_u32() as u8
    }
}