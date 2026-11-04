//! ID kind registry and state schema (spec "Core › State", "Module system",
//! ADR-001 §4).
//!
//! A [`StateSchema`] is built up by modules and the game, then
//! [frozen](StateSchema::freeze) into a [`FrozenSchema`] from which
//! [`State`](crate::State)s are created.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::error::{CoreError, Result};
use crate::ident::{IdKind, check_path, check_segment};
use crate::value::{Type, Value};

/// Who declared a namespace or ID kind: the game or a named module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(Option<String>);

impl Owner {
    /// The game itself (content or game code).
    pub fn game() -> Self {
        Self(None)
    }

    /// A module, by name. The name is one identifier segment (see
    /// [`crate::ident`]); `game` is reserved.
    pub fn module(name: &str) -> Result<Self> {
        check_segment("module name", name)?;
        if name == "game" {
            return Err(CoreError::InvalidName {
                what: "module name",
                text: name.to_owned(),
                reason: "`game` is reserved for the game itself",
            });
        }
        Ok(Self(Some(name.to_owned())))
    }

    /// The module name, or `None` for the game.
    pub fn module_name(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            None => f.write_str("the game"),
            Some(name) => write!(f, "module `{name}`"),
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
    /// even by the same owner.
    pub fn register(&mut self, kind: IdKind, owner: Owner) -> Result<()> {
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
}

/// A state schema under construction.
///
/// Declare namespaces, then keys in them; every check happens at declaration
/// time. ID kinds must all be registered in the [`KindRegistry`] passed to
/// [`new`](Self::new) before keys of type `set<kind>` are declared. Call
/// [`freeze`](Self::freeze) when done.
#[derive(Debug, Clone, Default)]
pub struct StateSchema {
    data: SchemaData,
}

impl StateSchema {
    /// An empty schema that checks `set<kind>` types against `kinds`.
    pub fn new(kinds: KindRegistry) -> Self {
        Self {
            data: SchemaData {
                kinds,
                ..SchemaData::default()
            },
        }
    }

    /// Declares namespace `name`, owned by `owner`. The name is one
    /// identifier segment. Declaring a namespace twice is an error.
    pub fn add_namespace(&mut self, name: &str, owner: Owner) -> Result<()> {
        check_segment("namespace", name)?;
        if let Some(existing) = self.data.namespaces.get(name) {
            return Err(CoreError::DuplicateNamespace {
                namespace: name.to_owned(),
                owner: existing.clone(),
            });
        }
        self.data.namespaces.insert(name.to_owned(), owner);
        Ok(())
    }

    /// Declares `key` with type `ty` and initial value `default`.
    ///
    /// Errors if the key's namespace isn't declared, the key is already
    /// declared, `ty` is a set of an unregistered kind, or `default` doesn't
    /// match `ty`.
    pub fn add_key(&mut self, key: Key, ty: Type, default: Value) -> Result<()> {
        if !self.data.namespaces.contains_key(key.namespace()) {
            return Err(CoreError::UnknownNamespace { key });
        }
        if self.data.keys.contains_key(&key) {
            return Err(CoreError::DuplicateKey { key });
        }
        if let Type::Set(kind) = &ty
            && !self.data.kinds.contains(kind)
        {
            return Err(CoreError::UnknownKind {
                key,
                kind: kind.clone(),
            });
        }
        if !default.type_matches(&ty) {
            return Err(CoreError::BadDefault {
                key,
                expected: ty,
                found: default,
            });
        }
        self.data.keys.insert(key, KeyDecl { ty, default });
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

    /// The type of `key`, or `None` if it isn't declared.
    pub fn key_type(&self, key: &Key) -> Option<&Type> {
        self.data.keys.get(key).map(|decl| &decl.ty)
    }

    /// The default value of `key`, or `None` if it isn't declared.
    pub fn default_value(&self, key: &Key) -> Option<&Value> {
        self.data.keys.get(key).map(|decl| &decl.default)
    }

    /// All declared keys in sorted order.
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
        for bad in [
            "travel",
            "travel.",
            ".mode",
            "Travel.mode",
            "travel..mode",
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
                kind: kind("npc")
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
                    found: default
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
        assert_eq!(listed, ["var.a", "var.b"]);
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
}
