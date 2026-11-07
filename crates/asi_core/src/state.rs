//! The state store (spec "Core › State").

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{CoreError, Result};
use crate::ident::{Id, IdKind};
use crate::schema::{FrozenSchema, Key};
use crate::value::{Type, Value};

/// The single typed state store.
///
/// Holds a value for every key in its [`FrozenSchema`], and only those keys.
/// Every write is checked against the key's type, so the store can never hold
/// a value its schema doesn't allow.
#[derive(Debug, Clone)]
pub struct State {
    schema: FrozenSchema,
    values: BTreeMap<Key, Value>,
}

impl State {
    /// A state with every key in `schema` at its default.
    pub fn new(schema: &FrozenSchema) -> Self {
        let values = schema
            .keys()
            .filter_map(|key| {
                schema
                    .default_value(key)
                    .map(|value| (key.clone(), value.clone()))
            })
            .collect();
        Self {
            schema: schema.clone(),
            values,
        }
    }

    /// The schema this state was created from.
    pub fn schema(&self) -> &FrozenSchema {
        &self.schema
    }

    /// The current value of `key`.
    pub fn get(&self, key: &Key) -> Result<&Value> {
        self.values
            .get(key)
            .ok_or_else(|| CoreError::UnknownKey { key: key.clone() })
    }

    /// Replaces the value of `key`. Errors if the key is unknown or `value`
    /// doesn't match its type (including enum symbols outside the type and
    /// set members of the wrong kind); the state is unchanged on error.
    pub fn set(&mut self, key: &Key, value: Value) -> Result<()> {
        let ty = self.key_type(key)?;
        if !value.type_matches(ty) {
            return Err(CoreError::TypeMismatch {
                key: key.clone(),
                expected: ty.clone(),
                found: Box::new(value),
            });
        }
        self.values.insert(key.clone(), value);
        Ok(())
    }

    /// Adds `id` to the set at `key`. Returns whether it was newly added.
    pub fn insert(&mut self, key: &Key, id: Id) -> Result<bool> {
        Ok(self.set_mut(key, &id)?.insert(id))
    }

    /// Removes `id` from the set at `key`. Returns whether it was present.
    pub fn remove(&mut self, key: &Key, id: &Id) -> Result<bool> {
        Ok(self.set_mut(key, id)?.remove(id))
    }

    /// Whether the set at `key` contains `id`.
    ///
    /// Asking with an ID of the wrong kind is an error, not `false`, so
    /// mistakes surface instead of silently failing.
    pub fn contains(&self, key: &Key, id: &Id) -> Result<bool> {
        let kind = self.check_set_kind(key, id)?;
        match self.get(key)? {
            Value::Set(members) => Ok(members.contains(id)),
            other => Err(broken_set(key, kind, other)),
        }
    }

    /// The declared type of `key`.
    fn key_type(&self, key: &Key) -> Result<&Type> {
        self.schema
            .key_type(key)
            .ok_or_else(|| CoreError::UnknownKey { key: key.clone() })
    }

    /// Checks that `key` is a set whose kind matches `id`; returns the kind.
    fn check_set_kind(&self, key: &Key, id: &Id) -> Result<IdKind> {
        match self.key_type(key)? {
            Type::Set(kind) if kind == id.kind() => Ok(kind.clone()),
            Type::Set(kind) => Err(CoreError::WrongIdKind {
                key: key.clone(),
                expected: kind.clone(),
                id: id.clone(),
            }),
            other => Err(CoreError::NotASet {
                key: key.clone(),
                found: other.clone(),
            }),
        }
    }

    /// The set at `key`, after checking `id` may go in it.
    fn set_mut(&mut self, key: &Key, id: &Id) -> Result<&mut BTreeSet<Id>> {
        let kind = self.check_set_kind(key, id)?;
        match self.values.get_mut(key) {
            Some(Value::Set(members)) => Ok(members),
            Some(other) => Err(broken_set(key, kind, other)),
            None => Err(CoreError::UnknownKey { key: key.clone() }),
        }
    }
}

/// Error for a `set<kind>` key whose stored value isn't a set.
///
/// [`State::set`] keeps every value matching its type, so this can't happen;
/// it exists so a broken invariant is reported instead of panicking.
fn broken_set(key: &Key, kind: IdKind, found: &Value) -> CoreError {
    CoreError::TypeMismatch {
        key: key.clone(),
        expected: Type::Set(kind),
        found: Box::new(found.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::Symbol;
    use crate::schema::{KindRegistry, Owner, StateSchema};
    use crate::time::{Duration, Time};

    fn key(s: &str) -> Key {
        s.parse().unwrap()
    }

    fn id(s: &str) -> Id {
        s.parse().unwrap()
    }

    fn sym(s: &str) -> Symbol {
        Symbol::new(s).unwrap()
    }

    fn mode() -> Type {
        Type::Enum([sym("walk"), sym("bicycle")].into())
    }

    fn state() -> State {
        let mut kinds = KindRegistry::new();
        kinds
            .register(IdKind::new("clue").unwrap(), Owner::game())
            .unwrap();
        let travel = Owner::module("travel").unwrap();
        kinds
            .register(IdKind::new("location").unwrap(), travel.clone())
            .unwrap();
        let mut schema = StateSchema::new(kinds);
        schema.add_namespace("var", Owner::game()).unwrap();
        schema.add_namespace("travel", travel).unwrap();
        let decls = [
            ("var.met", Type::Bool, Value::Bool(false)),
            ("var.trust", Type::Int, Value::Int(0)),
            (
                "var.clues",
                Type::Set(IdKind::new("clue").unwrap()),
                Value::Set([id("clue.start")].into()),
            ),
            ("travel.mode", mode(), Value::Enum(sym("walk"))),
            (
                "travel.visited",
                Type::Set(IdKind::new("location").unwrap()),
                Value::Set(BTreeSet::new()),
            ),
            ("var.deadline", Type::Time, Value::Time(Time::START)),
            ("var.delay", Type::Duration, Value::Duration(Duration::ZERO)),
        ];
        for (k, ty, default) in decls {
            schema.add_key(key(k), ty, default).unwrap();
        }
        State::new(&schema.freeze())
    }

    #[test]
    fn starts_at_defaults() {
        let state = state();
        assert_eq!(state.get(&key("var.met")), Ok(&Value::Bool(false)));
        assert_eq!(state.get(&key("var.trust")), Ok(&Value::Int(0)));
        assert_eq!(
            state.get(&key("travel.mode")),
            Ok(&Value::Enum(sym("walk")))
        );
        assert_eq!(
            state.get(&key("var.clues")),
            Ok(&Value::Set([id("clue.start")].into()))
        );
        for k in state.schema().keys() {
            assert_eq!(state.get(k).ok(), state.schema().default_value(k));
        }
    }

    #[test]
    fn get_rejects_unknown_keys() {
        assert_eq!(
            state().get(&key("var.nope")),
            Err(CoreError::UnknownKey {
                key: key("var.nope")
            })
        );
        assert!(state().get(&key("ghost.met")).is_err());
    }

    #[test]
    fn set_writes_matching_values() {
        let mut state = state();
        let deadline = Value::Time("d2 23:00".parse().unwrap());
        state.set(&key("var.trust"), Value::Int(-4)).unwrap();
        state
            .set(&key("travel.mode"), Value::Enum(sym("bicycle")))
            .unwrap();
        state.set(&key("var.deadline"), deadline.clone()).unwrap();
        assert_eq!(state.get(&key("var.trust")), Ok(&Value::Int(-4)));
        assert_eq!(
            state.get(&key("travel.mode")),
            Ok(&Value::Enum(sym("bicycle")))
        );
        assert_eq!(state.get(&key("var.deadline")), Ok(&deadline));
    }

    #[test]
    fn set_rejects_unknown_keys() {
        let mut state = state();
        assert_eq!(
            state.set(&key("var.nope"), Value::Int(1)),
            Err(CoreError::UnknownKey {
                key: key("var.nope")
            })
        );
        assert!(state.get(&key("var.nope")).is_err());
    }

    #[test]
    fn set_rejects_type_mismatches_and_leaves_state_unchanged() {
        let mut state = state();
        let cases = [
            ("var.trust", Value::Bool(true)),
            ("var.met", Value::Int(1)),
            ("var.deadline", Value::Duration(Duration::ZERO)),
            ("var.delay", Value::Time(Time::START)),
            ("travel.mode", Value::Enum(sym("car"))),
            ("travel.mode", Value::Int(0)),
            ("var.clues", Value::Set([id("location.diner")].into())),
            ("var.clues", Value::Int(0)),
        ];
        for (k, value) in cases {
            let before = state.get(&key(k)).unwrap().clone();
            let err = state.set(&key(k), value.clone()).unwrap_err();
            assert_eq!(
                err,
                CoreError::TypeMismatch {
                    key: key(k),
                    expected: state.schema().key_type(&key(k)).unwrap().clone(),
                    found: Box::new(value)
                }
            );
            assert_eq!(state.get(&key(k)), Ok(&before));
        }
    }

    #[test]
    fn enum_error_message() {
        let err = state()
            .set(&key("travel.mode"), Value::Enum(sym("car")))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "cannot store `:car` in key `travel.mode`: expected a value of type \
             `enum{:bicycle, :walk}`"
        );
    }

    #[test]
    fn set_operations() {
        let mut state = state();
        let clues = key("var.clues");
        assert_eq!(state.contains(&clues, &id("clue.receipt")), Ok(false));
        assert_eq!(state.insert(&clues, id("clue.receipt")), Ok(true));
        assert_eq!(state.insert(&clues, id("clue.receipt")), Ok(false));
        assert_eq!(state.contains(&clues, &id("clue.receipt")), Ok(true));
        assert_eq!(state.remove(&clues, &id("clue.start")), Ok(true));
        assert_eq!(state.remove(&clues, &id("clue.start")), Ok(false));
        assert_eq!(
            state.get(&clues),
            Ok(&Value::Set([id("clue.receipt")].into()))
        );
    }

    #[test]
    fn set_operations_reject_wrong_kinds() {
        let mut state = state();
        let clues = key("var.clues");
        let wrong = id("location.diner");
        let expected = CoreError::WrongIdKind {
            key: clues.clone(),
            expected: IdKind::new("clue").unwrap(),
            id: wrong.clone(),
        };
        assert_eq!(state.insert(&clues, wrong.clone()), Err(expected.clone()));
        assert_eq!(state.remove(&clues, &wrong), Err(expected.clone()));
        assert_eq!(state.contains(&clues, &wrong), Err(expected.clone()));
        assert_eq!(
            expected.to_string(),
            "key `var.clues` holds `set<clue>`, but `location.diner` is of kind `location`"
        );
        assert_eq!(
            state.get(&clues),
            Ok(&Value::Set([id("clue.start")].into()))
        );
    }

    #[test]
    fn set_operations_reject_non_sets_and_unknown_keys() {
        let mut state = state();
        let clue = id("clue.a");
        let not_a_set = CoreError::NotASet {
            key: key("var.trust"),
            found: Type::Int,
        };
        assert_eq!(
            state.insert(&key("var.trust"), clue.clone()),
            Err(not_a_set.clone())
        );
        assert_eq!(
            state.remove(&key("var.trust"), &clue),
            Err(not_a_set.clone())
        );
        assert_eq!(state.contains(&key("var.trust"), &clue), Err(not_a_set));
        let unknown = CoreError::UnknownKey {
            key: key("var.nope"),
        };
        assert_eq!(
            state.insert(&key("var.nope"), clue.clone()),
            Err(unknown.clone())
        );
        assert_eq!(state.remove(&key("var.nope"), &clue), Err(unknown.clone()));
        assert_eq!(state.contains(&key("var.nope"), &clue), Err(unknown));
    }

    #[test]
    fn set_iteration_is_ordered() {
        let mut state = state();
        let visited = key("travel.visited");
        for loc in ["location.zoo", "location.bar", "location.diner.back"] {
            state.insert(&visited, id(loc)).unwrap();
        }
        let Ok(Value::Set(members)) = state.get(&visited) else {
            panic!("not a set");
        };
        let listed: Vec<String> = members.iter().map(ToString::to_string).collect();
        assert_eq!(
            listed,
            ["location.bar", "location.diner.back", "location.zoo"]
        );
    }

    #[test]
    fn core_time_is_in_every_state() {
        let mut state = state();
        assert_eq!(state.get(&Key::core_time()), Ok(&Value::Time(Time::START)));
        let later = Value::Time("d3 22:00".parse().unwrap());
        state.set(&Key::core_time(), later.clone()).unwrap();
        assert_eq!(state.get(&Key::core_time()), Ok(&later));
        assert!(state.set(&Key::core_time(), Value::Int(0)).is_err());
    }

    #[test]
    fn single_id_writes_are_checked() {
        let mut kinds = KindRegistry::new();
        for k in ["npc", "clue"] {
            kinds
                .register(IdKind::new(k).unwrap(), Owner::game())
                .unwrap();
        }
        let mut schema = StateSchema::new(kinds);
        schema.add_namespace("case", Owner::game()).unwrap();
        let npc = Type::Ref(IdKind::new("npc").unwrap());
        schema
            .add_key(key("case.accused"), npc.clone(), Value::Ref(None))
            .unwrap();
        let mut state = State::new(&schema.freeze());
        let accused = key("case.accused");
        assert_eq!(state.get(&accused), Ok(&Value::Ref(None)));

        let carla = Value::Ref(Some(id("npc.carla")));
        state.set(&accused, carla.clone()).unwrap();
        assert_eq!(state.get(&accused), Ok(&carla));
        state.set(&accused, Value::Ref(None)).unwrap();
        assert_eq!(state.get(&accused), Ok(&Value::Ref(None)));

        for bad in [
            Value::Ref(Some(id("clue.carla"))),
            Value::Set([id("npc.carla")].into()),
            Value::Bool(true),
        ] {
            let err = state.set(&accused, bad.clone()).unwrap_err();
            assert_eq!(
                err,
                CoreError::TypeMismatch {
                    key: accused.clone(),
                    expected: npc.clone(),
                    found: Box::new(bad)
                }
            );
        }
        assert_eq!(state.get(&accused), Ok(&Value::Ref(None)));
        // Set operations don't apply to single-ID keys.
        assert_eq!(
            state.insert(&accused, id("npc.carla")),
            Err(CoreError::NotASet {
                key: accused.clone(),
                found: npc
            })
        );
        assert_eq!(
            CoreError::TypeMismatch {
                key: accused,
                expected: Type::Ref(IdKind::new("npc").unwrap()),
                found: Box::new(Value::Ref(Some(id("clue.carla"))))
            }
            .to_string(),
            "cannot store `clue.carla` in key `case.accused`: expected a value of type `id<npc>`"
        );
    }

    #[test]
    fn clones_are_independent() {
        let original = state();
        let mut copy = original.clone();
        copy.set(&key("var.met"), Value::Bool(true)).unwrap();
        assert_eq!(original.get(&key("var.met")), Ok(&Value::Bool(false)));
    }
}
