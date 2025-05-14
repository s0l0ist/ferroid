use ferroid::{IdGenStatus, Result, Snowflake, SnowflakeGenerator};
use std::collections::VecDeque;
use std::marker::PhantomData;

pub struct Army<G, ID>
where
    G: SnowflakeGenerator<ID>,
    ID: Snowflake,
{
    generators: Vec<G>,
    ready: VecDeque<usize>,
    pending: VecDeque<usize>,
    _id: PhantomData<ID>,
}

impl<G, ID> Army<G, ID>
where
    G: SnowflakeGenerator<ID>,
    ID: Snowflake,
{
    pub fn new(generators: Vec<G>) -> Self {
        let ready = (0..generators.len()).collect();
        Self {
            generators,
            ready,
            pending: VecDeque::new(),
            _id: PhantomData,
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
