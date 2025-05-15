use ferroid::{IdGenStatus, Result, Snowflake, SnowflakeGenerator, TimeSource};
use std::collections::VecDeque;
use std::marker::PhantomData;

pub struct Army<G, ID, T>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    generators: Vec<G>,
    ready: VecDeque<usize>,
    pending: VecDeque<usize>,
    _id: PhantomData<ID>,
    _t: PhantomData<T>,
}

impl<G, ID, T> Army<G, ID, T>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    pub fn new(generators: Vec<G>) -> Self {
        Self {
            ready: (0..generators.len()).collect(),
            generators,
            pending: VecDeque::new(),
            _id: PhantomData,
            _t: PhantomData,
        }
    }

    pub fn next_id(&mut self) -> ID {
        self.try_next_id().unwrap()
    }

    pub fn try_next_id(&mut self) -> Result<ID> {
        loop {
            let len = self.ready.len();

            for _ in 0..len {
                let idx = self
                    .ready
                    .pop_front()
                    .expect("ready queue empty during poll");

                match self.generators[idx].try_next() {
                    Ok(IdGenStatus::Ready { id }) => {
                        self.ready.push_back(idx);
                        return Ok(id);
                    }
                    Ok(IdGenStatus::Pending { .. }) => {
                        self.pending.push_back(idx);
                    }
                    Err(e) => {
                        self.pending.push_back(idx);
                        return Err(e);
                    }
                }
            }

            std::mem::swap(&mut self.ready, &mut self.pending);
        }
    }
}
