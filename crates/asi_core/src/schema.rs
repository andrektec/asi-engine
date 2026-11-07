//! ID kind registry and state schema (spec "Core › State", "Module system",
//! ADR-001 §4, ADR-004).
//!
//! A [`StateSchema`] is built up by modules and the game, then
//! [frozen](StateSchema::freeze) into a [`FrozenSchema`] from which
//! [`State`](crate::State)s are created.
//!
//! Every schema contains the engine's namespace `core` with the key
//! `core.time` (the current game time, type `time`, default `d1 00:00`). The
//! name `core` is reserved: it can't be declared as a namespace, registered
//! as an ID kind or used as a module name (ADR-004 §2).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::error::{CoreError, Result};
use crate::ident::{Id, IdKind, check_path, check_segment};
use crate::time::Time;
use crate::value::{Type, Value};

/// The engine's reserved namespace (ADR-004 §2).
const CORE: &str = "core";

/// The error for using the reserved name `core` as `what`.
fn reserved_core(what: &'static str) -> CoreError {
    CoreError::InvalidName {
        what,
        text: CORE.to_owned(),
        reason: "`core` is reserved for the engine",
    }
}

/// Who declared a namespace, ID kind or function: the engine, the game or a
/// named module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(OwnerKind);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum OwnerKind {
    Engine,
    Game,
    Module(String),
}

impl Owner {
    /// The engine itself, for what the engine declares on its own (the
    /// `core` namespace and core functions). Modules and games use
    /// [`module`](Self::module) and [`game`](Self::game).
    pub fn engine() -> Self {
        Self(OwnerKind::Engine)
    }

    /// The game itself (content or game code).
    pub fn game() -> Self {
        Self(OwnerKind::Game)
    }

    /// A module, by name. The name is one identifier segment (see
    /// [`crate::ident`]); `game` and `core` are reserved.
    pub fn module(name: &str) -> Result<Self> {
        check_segment("module name", name)?;
        if name == "game" {
            return Err(CoreError::InvalidName {
                what: "module name",
                text: name.to_owned(),
                reason: "`game` is reserved for the game itself",
            });
        }
        if name == CORE {
            return Err(reserved_core("module name"));
        }
        Ok(Self(OwnerKind::Module(name.to_owned())))
    }

    /// The module name, or `None` for the game or the engine.
    pub fn module_name(&self) -> Option<&str> {
        match &self.0 {
            OwnerKind::Module(name) => Some(name),
            OwnerKind::Engine | OwnerKind::Game => None,
        }
    }

    /// Whether this is the engine.
    pub fn is_engine(&self) -> bool {
        self.0 == OwnerKind::Engine
    }
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            OwnerKind::Engine => f.write_str("the engine"),
            OwnerKind::Game => f.write_str("the game"),
            OwnerKind::Module(name) => write!(f, "module `{name}`"),
        }
    }
}

/// The registered ID kinds and who registered each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KindRegistry {
    kinds: BTreeMap<IdKind, Owner>,
}

impl KindRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `kind` for `owner`. Registering a kind twice is an error,
    /// even by the same owner, and so is registering `core`, which is
    /// reserved for the engine's namespace.
    pub fn register(&mut self, kind: IdKind, owner: Owner) -> Result<()> {
        if kind.as_str() == CORE {
            return Err(reserved_core("ID kind"));
        }
        if let Some(existing) = self.kinds.get(&kind) {
            return Err(CoreError::DuplicateKind {
                kind,
                owner: existing.clone(),
            });
        }
        self.kinds.insert(kind, owner);
        Ok(())
    }

    /// Whether `kind` is registered.
    pub fn contains(&self, kind: &IdKind) -> bool {
        self.kinds.contains_key(kind)
    }

    /// Who registered `kind`, if anyone.
    pub fn owner(&self, kind: &IdKind) -> Option<&Owner> {
        self.kinds.get(kind)
    }

    /// All kinds in sorted order, with their owners.
    pub fn iter(&self) -> impl Iterator<Item = (&IdKind, &Owner)> {
        self.kinds.iter()
    }
}

/// A state key, `namespace.name` (e.g. `travel.mode`).
///
/// The namespace is one identifier segment; the name is a dotted path of one
/// or more segments (see [`crate::ident`]), so `travel.npc.carla` is a key in
/// namespace `travel` named `npc.carla`. Ordered by namespace, then name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    namespace: String,
    name: String,
}

impl Key {
    /// Creates a key from its namespace and name.
    pub fn new(namespace: &str, name: &str) -> Result<Self> {
        check_segment("namespace", namespace)?;
        check_path("state key", name)?;
        Ok(Self {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        })
    }

    /// The namespace, e.g. `travel`.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The name within the namespace, e.g. `mode`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `core.time`, the current game time, present in every schema.
    pub fn core_time() -> Self {
        Self {
            namespace: CORE.to_owned(),
            name: "time".to_owned(),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.name)
    }
}

impl FromStr for Key {
    type Err = CoreError;

    /// Parses `namespace.name`.
    fn from_str(s: &str) -> Result<Self> {
        check_path("state key", s)?;
        match s.split_once('.') {
            Some((namespace, name)) => Self::new(namespace, name),
            None => Err(CoreError::InvalidName {
                what: "state key",
                text: s.to_owned(),
                reason: "must be `namespace.name`",
            }),
        }
    }
}

/// A declared key's type and default.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyDecl {
    ty: Type,
    default: Value,
}

/// The schema contents shared by builder and frozen schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SchemaData {
    kinds: KindRegistry,
    namespaces: BTreeMap<String, Owner>,
    keys: BTreeMap<Key, KeyDecl>,
    holders: BTreeMap<IdKind, Key>,
}

/// A state schema under construction.
///
/// Declare namespaces, then keys in them; every check happens at declaration
/// time. ID kinds must all be registered in the [`KindRegistry`] passed to
/// [`new`](Self::new) before keys of type `set<kind>` or `id<kind>` are
/// declared. Call [`freeze`](Self::freeze) when done.
#[derive(Debug, Clone)]
pub struct StateSchema {
    data: SchemaData,
}

impl Default for StateSchema {
    /// A schema with no ID kinds, holding only `core.time`.
    fn default() -> Self {
        Self::new(KindRegistry::new())
    }
}

impl StateSchema {
    /// A schema that checks `set<kind>` and `id<kind>` types against
    /// `kinds`. It starts with the engine's namespace `core` and the key
    /// `core.time` (type `time`, default `d1 00:00`).
    pub fn new(kinds: KindRegistry) -> Self {
        let mut data = SchemaData {
            kinds,
            ..SchemaData::default()
        };
        data.namespaces.insert(CORE.to_owned(), Owner::engine());
        data.keys.insert(
            Key::core_time(),
            KeyDecl {
                ty: Type::Time,
                default: Value::Time(Time::START),
            },
        );
        Self { data }
    }

    /// Declares namespace `name`, owned by `owner`. The name is one
    /// identifier segment. Declaring a namespace twice is an error, and so
    /// is using the name of a registered ID kind (namespaces and ID kinds
    /// share one name space) or `core`, which is reserved for the engine.
    pub fn add_namespace(&mut self, name: &str, owner: Owner) -> Result<()> {
        check_segment("namespace", name)?;
        if name == CORE {
            return Err(reserved_core("namespace"));
        }
        if let Some(existing) = self.data.namespaces.get(name) {
            return Err(CoreError::DuplicateNamespace {
                namespace: name.to_owned(),
                owner: existing.clone(),
            });
        }
        if let Some(kind_owner) = IdKind::new(name)
            .ok()
            .and_then(|kind| self.data.kinds.owner(&kind))
        {
            return Err(CoreError::NamespaceIsKind {
                namespace: name.to_owned(),
                owner: kind_owner.clone(),
            });
        }
        self.data.namespaces.insert(name.to_owned(), owner);
        Ok(())
    }

    /// Declares `key` with type `ty` and initial value `default`.
    ///
    /// Errors if the key's namespace isn't declared, the key is already
    /// declared, `ty` is a `set<kind>` or `id<kind>` of an unregistered
    /// kind, or `default` doesn't match `ty`. Keys can't be added to the
    /// engine's namespace `core`.
    pub fn add_key(&mut self, key: Key, ty: Type, default: Value) -> Result<()> {
        if key.namespace() == CORE {
            return Err(reserved_core("namespace"));
        }
        if !self.data.namespaces.contains_key(key.namespace()) {
            return Err(CoreError::UnknownNamespace { key });
        }
        if self.data.keys.contains_key(&key) {
            return Err(CoreError::DuplicateKey { key });
        }
        if let Type::Set(kind) | Type::Ref(kind) = &ty
            && !self.data.kinds.contains(kind)
        {
            let kind = kind.clone();
            return Err(CoreError::UnknownKind { key, kind, ty });
        }
        if !default.type_matches(&ty) {
            return Err(CoreError::BadDefault {
                key,
                expected: ty,
                found: Box::new(default),
            });
        }
        self.data.keys.insert(key, KeyDecl { ty, default });
        Ok(())
    }

    /// Marks the declared `set<kind>` key `key` as the holder of `kind`: the
    /// set that `has(id)` tests for IDs of that kind (ADR-004 §1).
    ///
    /// Errors if `key` isn't declared, isn't a set, or its kind already has a
    /// holder (including `key` itself).
    pub fn set_holder(&mut self, key: &Key) -> Result<()> {
        let kind = match self.data.keys.get(key) {
            None => return Err(CoreError::UnknownKey { key: key.clone() }),
            Some(KeyDecl {
                ty: Type::Set(kind),
                ..
            }) => kind.clone(),
            Some(decl) => {
                return Err(CoreError::NotASet {
                    key: key.clone(),
                    found: decl.ty.clone(),
                });
            }
        };
        if let Some(existing) = self.data.holders.get(&kind) {
            return Err(CoreError::DuplicateHolder {
                kind,
                existing: existing.clone(),
                key: key.clone(),
            });
        }
        self.data.holders.insert(kind, key.clone());
        Ok(())
    }

    /// Finishes the schema. A frozen schema can't change.
    pub fn freeze(self) -> FrozenSchema {
        FrozenSchema {
            data: Arc::new(self.data),
        }
    }
}

/// A finished, immutable state schema. Cheap to clone (shared).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenSchema {
    data: Arc<SchemaData>,
}

impl FrozenSchema {
    /// The registered ID kinds.
    pub fn kinds(&self) -> &KindRegistry {
        &self.data.kinds
    }

    /// Who owns namespace `name`, or `None` if it isn't declared.
    pub fn namespace_owner(&self, name: &str) -> Option<&Owner> {
        self.data.namespaces.get(name)
    }

    /// All declared namespaces in sorted order, with their owners. Includes
    /// `core`.
    pub fn namespaces(&self) -> impl Iterator<Item = (&str, &Owner)> {
        self.data
            .namespaces
            .iter()
            .map(|(name, owner)| (name.as_str(), owner))
    }

    /// The holder set of `kind` (see [`StateSchema::set_holder`]), or
    /// `None` if the kind has none.
    pub fn holder(&self, kind: &IdKind) -> Option<&Key> {
        self.data.holders.get(kind)
    }

    /// The holder set that `has(id)` tests for `id`, or `None` if its kind
    /// has none.
    pub fn holder_of(&self, id: &Id) -> Option<&Key> {
        self.holder(id.kind())
    }

    /// The type of `key`, or `None` if it isn't declared.
    pub fn key_type(&self, key: &Key) -> Option<&Type> {
        self.data.keys.get(key).map(|decl| &decl.ty)
    }

    /// The default value of `key`, or `None` if it isn't declared.
    pub fn default_value(&self, key: &Key) -> Option<&Value> {
        self.data.keys.get(key).map(|decl| &decl.default)
    }

    /// All declared keys in sorted order, including `core.time`.
    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.data.keys.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::Symbol;
    use std::collections::BTreeSet;

    fn key(s: &str) -> Key {
        s.parse().unwrap()
    }

    fn kind(s: &str) -> IdKind {
        IdKind::new(s).unwrap()
    }

    fn kinds() -> KindRegistry {
        let mut kinds = KindRegistry::new();
        kinds.register(kind("clue"), Owner::game()).unwrap();
        kinds
    }

    fn schema() -> StateSchema {
        let mut schema = StateSchema::new(kinds());
        schema.add_namespace("var", Owner::game()).unwrap();
        schema
    }

    #[test]
    fn owner_names() {
        assert_eq!(Owner::game().module_name(), None);
        assert_eq!(
            Owner::module("travel").unwrap().module_name(),
            Some("travel")
        );
        assert!(Owner::module("game").is_err());
        assert!(Owner::module("Travel").is_err());
        assert!(Owner::module("a.b").is_err());
        assert_eq!(Owner::game().to_string(), "the game");
        assert_eq!(
            Owner::module("travel").unwrap().to_string(),
            "module `travel`"
        );
    }

    #[test]
    fn kind_registry_rejects_duplicates() {
        let mut kinds = kinds();
        let travel = Owner::module("travel").unwrap();
        kinds.register(kind("location"), travel.clone()).unwrap();
        assert!(kinds.contains(&kind("location")));
        assert_eq!(kinds.owner(&kind("location")), Some(&travel));
        assert!(!kinds.contains(&kind("npc")));
        assert_eq!(
            kinds.register(kind("clue"), travel),
            Err(CoreError::DuplicateKind {
                kind: kind("clue"),
                owner: Owner::game()
            })
        );
        assert!(kinds.register(kind("location"), Owner::game()).is_err());
        let listed: Vec<&str> = kinds.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(listed, ["clue", "location"]);
    }

    #[test]
    fn keys_parse_and_display() {
        let k = key("travel.mode");
        assert_eq!((k.namespace(), k.name()), ("travel", "mode"));
        assert_eq!(k.to_string(), "travel.mode");
        let nested = key("travel.npc.carla");
        assert_eq!((nested.namespace(), nested.name()), ("travel", "npc.carla"));
        assert_eq!(nested, Key::new("travel", "npc.carla").unwrap());
        let numbered = key("dialogue.carla.042");
        assert_eq!(numbered.name(), "carla.042");
        assert_eq!(numbered.to_string(), "dialogue.carla.042");
        assert_eq!(key("dialogue.042").name(), "042");
        for bad in [
            "travel",
            "travel.",
            ".mode",
            "Travel.mode",
            "travel..mode",
            "1travel.mode",
            "042.mode",
            "travel._mode",
            "",
        ] {
            assert!(bad.parse::<Key>().is_err(), "{bad}");
        }
        assert!(Key::new("a.b", "c").is_err());
    }

    #[test]
    fn namespaces_reject_duplicates_and_bad_names() {
        let mut schema = schema();
        let travel = Owner::module("travel").unwrap();
        schema.add_namespace("travel", travel.clone()).unwrap();
        assert_eq!(
            schema.add_namespace("travel", Owner::game()),
            Err(CoreError::DuplicateNamespace {
                namespace: "travel".to_owned(),
                owner: travel
            })
        );
        assert!(schema.add_namespace("a.b", Owner::game()).is_err());
        assert!(schema.add_namespace("", Owner::game()).is_err());
        assert!(schema.add_namespace("1st", Owner::game()).is_err());
    }

    #[test]
    fn namespaces_cannot_reuse_id_kind_names() {
        let mut schema = schema();
        let err = schema
            .add_namespace("clue", Owner::module("travel").unwrap())
            .unwrap_err();
        assert_eq!(
            err,
            CoreError::NamespaceIsKind {
                namespace: "clue".to_owned(),
                owner: Owner::game()
            }
        );
        assert_eq!(
            err.to_string(),
            "namespace `clue` has the same name as an ID kind registered by the game; \
             namespaces and ID kinds share one name space"
        );
        assert_eq!(schema.clone().freeze().namespace_owner("clue"), None);
    }

    #[test]
    fn keys_need_a_declared_namespace() {
        let mut schema = schema();
        assert_eq!(
            schema.add_key(key("travel.mode"), Type::Int, Value::Int(0)),
            Err(CoreError::UnknownNamespace {
                key: key("travel.mode")
            })
        );
    }

    #[test]
    fn keys_reject_duplicates() {
        let mut schema = schema();
        schema
            .add_key(key("var.trust"), Type::Int, Value::Int(0))
            .unwrap();
        assert_eq!(
            schema.add_key(key("var.trust"), Type::Bool, Value::Bool(false)),
            Err(CoreError::DuplicateKey {
                key: key("var.trust")
            })
        );
    }

    #[test]
    fn set_types_need_a_registered_kind() {
        let mut schema = schema();
        assert_eq!(
            schema.add_key(
                key("var.met"),
                Type::Set(kind("npc")),
                Value::Set(BTreeSet::new())
            ),
            Err(CoreError::UnknownKind {
                key: key("var.met"),
                kind: kind("npc"),
                ty: Type::Set(kind("npc"))
            })
        );
        schema
            .add_key(
                key("var.clues"),
                Type::Set(kind("clue")),
                Value::Set(BTreeSet::new()),
            )
            .unwrap();
    }

    #[test]
    fn defaults_must_match_the_type() {
        let mut schema = schema();
        let mode = Type::Enum([Symbol::new("walk").unwrap()].into());
        let cases = [
            (Type::Int, Value::Bool(false)),
            (mode.clone(), Value::Enum(Symbol::new("car").unwrap())),
            (
                Type::Enum(BTreeSet::new()),
                Value::Enum(Symbol::new("walk").unwrap()),
            ),
            (
                Type::Set(kind("clue")),
                Value::Set(["npc.a".parse().unwrap()].into()),
            ),
        ];
        for (ty, default) in cases {
            let err = schema
                .add_key(key("var.x"), ty.clone(), default.clone())
                .unwrap_err();
            assert_eq!(
                err,
                CoreError::BadDefault {
                    key: key("var.x"),
                    expected: ty,
                    found: Box::new(default)
                }
            );
        }
        schema
            .add_key(
                key("var.x"),
                mode,
                Value::Enum(Symbol::new("walk").unwrap()),
            )
            .unwrap();
    }

    #[test]
    fn frozen_schema_exposes_declarations() {
        let mut schema = schema();
        schema
            .add_key(key("var.b"), Type::Int, Value::Int(7))
            .unwrap();
        schema
            .add_key(key("var.a"), Type::Bool, Value::Bool(true))
            .unwrap();
        let frozen = schema.freeze();
        assert_eq!(frozen.key_type(&key("var.b")), Some(&Type::Int));
        assert_eq!(frozen.default_value(&key("var.b")), Some(&Value::Int(7)));
        assert_eq!(frozen.key_type(&key("var.zzz")), None);
        assert_eq!(frozen.namespace_owner("var"), Some(&Owner::game()));
        assert_eq!(frozen.namespace_owner("travel"), None);
        assert!(frozen.kinds().contains(&kind("clue")));
        let listed: Vec<String> = frozen.keys().map(ToString::to_string).collect();
        assert_eq!(listed, ["core.time", "var.a", "var.b"]);
    }

    #[test]
    fn errors_have_useful_messages() {
        let mut schema = schema();
        let err = schema
            .add_key(key("var.x"), Type::Int, Value::Bool(true))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "default `true` for key `var.x` does not match its type `int`"
        );
        let err = schema
            .add_key(
                key("var.y"),
                Type::Set(kind("npc")),
                Value::Set(BTreeSet::new()),
            )
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "key `var.y` has type `set<npc>`, but ID kind `npc` is not registered"
        );
    }

    #[test]
    fn every_schema_has_core_time() {
        for frozen in [schema().freeze(), StateSchema::default().freeze()] {
            assert_eq!(frozen.namespace_owner("core"), Some(&Owner::engine()));
            assert_eq!(frozen.key_type(&Key::core_time()), Some(&Type::Time));
            assert_eq!(
                frozen.default_value(&Key::core_time()),
                Some(&Value::Time(Time::START))
            );
        }
        assert_eq!(Key::core_time(), key("core.time"));
        let frozen = schema().freeze();
        let namespaces: Vec<&str> = frozen.namespaces().map(|(n, _)| n).collect();
        assert_eq!(namespaces, ["core", "var"]);
    }

    #[test]
    fn core_is_reserved() {
        let reserved = |what| CoreError::InvalidName {
            what,
            text: "core".to_owned(),
            reason: "`core` is reserved for the engine",
        };
        let mut schema = schema();
        assert_eq!(
            schema.add_namespace("core", Owner::game()),
            Err(reserved("namespace"))
        );
        assert_eq!(
            schema.add_key(key("core.other"), Type::Int, Value::Int(0)),
            Err(reserved("namespace"))
        );
        assert_eq!(
            schema.add_key(key("core.time"), Type::Int, Value::Int(0)),
            Err(reserved("namespace"))
        );
        assert_eq!(Owner::module("core"), Err(reserved("module name")));
        assert_eq!(
            KindRegistry::new().register(kind("core"), Owner::game()),
            Err(reserved("ID kind"))
        );
        assert_eq!(
            reserved("namespace").to_string(),
            "invalid namespace `core`: `core` is reserved for the engine"
        );
    }

    #[test]
    fn engine_owner() {
        assert!(Owner::engine().is_engine());
        assert!(!Owner::game().is_engine());
        assert_eq!(Owner::engine().module_name(), None);
        assert_eq!(Owner::engine().to_string(), "the engine");
    }

    #[test]
    fn holders() {
        let mut schema = schema();
        schema.add_namespace("game", Owner::game()).unwrap();
        let clue_set = Type::Set(kind("clue"));
        let empty = Value::Set(BTreeSet::new());
        schema
            .add_key(key("game.clues"), clue_set.clone(), empty.clone())
            .unwrap();
        schema
            .add_key(key("game.other_clues"), clue_set, empty)
            .unwrap();
        schema
            .add_key(key("game.count"), Type::Int, Value::Int(0))
            .unwrap();

        assert_eq!(
            schema.set_holder(&key("game.nope")),
            Err(CoreError::UnknownKey {
                key: key("game.nope")
            })
        );
        assert_eq!(
            schema.set_holder(&key("game.count")),
            Err(CoreError::NotASet {
                key: key("game.count"),
                found: Type::Int
            })
        );
        schema.set_holder(&key("game.clues")).unwrap();
        let err = schema.set_holder(&key("game.other_clues")).unwrap_err();
        assert_eq!(
            err,
            CoreError::DuplicateHolder {
                kind: kind("clue"),
                existing: key("game.clues"),
                key: key("game.other_clues")
            }
        );
        assert_eq!(
            err.to_string(),
            "cannot make `game.other_clues` the holder of ID kind `clue`: \
             `game.clues` already is"
        );
        assert!(schema.set_holder(&key("game.clues")).is_err());

        let frozen = schema.freeze();
        assert_eq!(frozen.holder(&kind("clue")), Some(&key("game.clues")));
        assert_eq!(frozen.holder(&kind("npc")), None);
        assert_eq!(
            frozen.holder_of(&"clue.a".parse().unwrap()),
            Some(&key("game.clues"))
        );
    }

    #[test]
    fn single_id_keys() {
        let mut kinds = kinds();
        kinds.register(kind("npc"), Owner::game()).unwrap();
        let mut schema = StateSchema::new(kinds);
        schema.add_namespace("case", Owner::game()).unwrap();
        let npc = Type::Ref(kind("npc"));
        schema
            .add_key(key("case.accused"), npc.clone(), Value::Ref(None))
            .unwrap();
        schema
            .add_key(
                key("case.first"),
                npc.clone(),
                Value::Ref(Some("npc.carla".parse().unwrap())),
            )
            .unwrap();
        let wrong = Value::Ref(Some("clue.carla".parse().unwrap()));
        assert_eq!(
            schema.add_key(key("case.x"), npc.clone(), wrong.clone()),
            Err(CoreError::BadDefault {
                key: key("case.x"),
                expected: npc.clone(),
                found: Box::new(wrong)
            })
        );
        assert!(
            schema
                .add_key(key("case.y"), npc, Value::Set(BTreeSet::new()))
                .is_err()
        );
        let err = schema
            .add_key(key("case.z"), Type::Ref(kind("place")), Value::Ref(None))
            .unwrap_err();
        assert_eq!(
            err,
            CoreError::UnknownKind {
                key: key("case.z"),
                kind: kind("place"),
                ty: Type::Ref(kind("place"))
            }
        );
        assert_eq!(
            err.to_string(),
            "key `case.z` has type `id<place>`, but ID kind `place` is not registered"
        );
        // A single-ID key can't be a holder.
        assert!(matches!(
            schema.set_holder(&key("case.accused")),
            Err(CoreError::NotASet { .. })
        ));
    }
}
