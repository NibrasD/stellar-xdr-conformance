//! Returns one SCMap per corpus shape. Every map has two keys that are
//! distinct ScVals (so the host accepts the map), but whose values become
//! equal keys in some languages' native maps.
//!
//! The return value lands in the transaction meta, so a single invocation
//! puts the shape into a real `LedgerCloseMeta`; nothing is stored in
//! contract state.
#![no_std]

use soroban_sdk::{contract, contractimpl, Env, IntoVal, Map, String, Symbol, Val};

#[contract]
pub struct KeyCollide;

#[contractimpl]
impl KeyCollide {
    /// Shape `n` matches corpus entry `SCVAL-MAP-KEYCOLLIDE-000n`.
    pub fn shape(env: Env, n: u32) -> Map<Val, Val> {
        let ten: Val = 10u32.into_val(&env);
        let twenty: Val = 20u32.into_val(&env);
        let mut m: Map<Val, Val> = Map::new(&env);
        match n {
            1 => {
                m.set(1u32.into_val(&env), ten);
                m.set(Symbol::new(&env, "1").into_val(&env), twenty);
            }
            2 => {
                m.set(String::from_str(&env, "k").into_val(&env), ten);
                m.set(Symbol::new(&env, "k").into_val(&env), twenty);
            }
            3 => {
                m.set(true.into_val(&env), ten);
                m.set(1u32.into_val(&env), twenty);
            }
            4 => {
                m.set(1u32.into_val(&env), ten);
                m.set(1i64.into_val(&env), twenty);
            }
            _ => panic!("unknown shape"),
        }
        m
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn every_shape_keeps_two_distinct_keys() {
        let env = Env::default();
        let id = env.register(KeyCollide, ());
        let client = KeyCollideClient::new(&env, &id);
        for n in 1..=4u32 {
            assert_eq!(client.shape(&n).len(), 2, "shape {n}");
        }
    }
}
