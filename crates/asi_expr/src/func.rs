//! Functions that conditions can call (ADR-004 §5).
//!
//! Modules and the engine register functions in a [`FunctionRegistry`].
//! Each [`Function`] declares its argument types, return type and the state
//! keys it reads; its code is a plain Rust function that gets the argument
//! values and a [`StateView`] limited to those keys.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use asi_core::ident::{name_segment_error, path_segment_error};
use asi_core::{CoreError, Id, Key, Owner, State, Type, Value};

/// The code of a [`Function`].
///
/// It gets the argument values, already checked against the declared
/// argument types, and a read-only view of the state limited to the
/// function's declared reads. It must be pure: no clock, randomness, IO or
/// global state, only its arguments and the view. A plain `fn` pointer (not a
/// closure) so it can't capture anything either.
pub type FunctionCode = fn(&[Value], &StateView<'_>) -> Result<Value, FunctionError>;

/// A function callable from conditions.
#[derive(Debug, Clone)]
pub struct Function {
    /// The dotted name: `module.name` for a module's functions
    /// (`rolls.passed`), one word for the engine's (`hour`).
    pub name: String,
    /// Who registers it: a module or the engine.
    pub owner: Owner,
    /// The argument types, in order. An `id<kind>` argument accepts an ID of
    /// that kind written out (`check.empathy_01`), `none`, or an `id<kind>`
    /// value; the code receives a [`Value::Ref`].
    pub args: Vec<Type>,
    /// The return type. Returning a value of another type is an evaluation
    /// error.
    pub ret: Type,
    /// The state keys the code may read.
    pub reads: BTreeSet<Key>,
    /// The code.
    pub code: FunctionCode,
}

impl Function {
    /// Whether `other` has the same name, argument types, return type and
    /// reads (the code isn't compared).
    pub(crate) fn same_signature(&self, other: &Self) -> bool {
        self.name == other.name
            && self.args == other.args
            && self.ret == other.ret
            && self.reads == other.reads
    }
}

/// The registered functions, by name.
#[derive(Debug, Clone, Default)]
pub struct FunctionRegistry {
    functions: BTreeMap<String, Function>,
}

impl FunctionRegistry {
    /// An empty registry, without even the core functions.
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with the engine's core functions (ADR-004 §3):
    ///
    /// - `day(time) -> int`: the day, starting at 1.
    /// - `hour(time) -> int`: the hour of the day, 0–23.
    /// - `minute(time) -> int`: the minute of the hour, 0–59.
    pub fn with_core() -> Self {
        let mut registry = Self::new();
        let core: [(&str, FunctionCode); 3] = [
            ("day", core_day),
            ("hour", core_hour),
            ("minute", core_minute),
        ];
        for (name, code) in core {
            registry.functions.insert(
                name.to_owned(),
                Function {
                    name: name.to_owned(),
                    owner: Owner::engine(),
                    args: vec![Type::Time],
                    ret: Type::Int,
                    reads: BTreeSet::new(),
                    code,
                },
            );
        }
        registry
    }

    /// Registers `function`.
    ///
    /// Errors if the name is taken, or doesn't fit the owner: a module's
    /// function names are the module name, a dot and a dotted path
    /// (`rolls.passed`); the engine's are one name segment (`hour`). `has`
    /// is reserved for the built-in form, and the game can't register
    /// functions.
    pub fn register(&mut self, function: Function) -> Result<(), RegistryError> {
        if let Some(reason) = name_problem(&function.name, &function.owner) {
            return Err(RegistryError::InvalidName {
                name: function.name,
                owner: function.owner,
                reason,
            });
        }
        if let Some(existing) = self.functions.get(&function.name) {
            return Err(RegistryError::Duplicate {
                name: function.name,
                owner: existing.owner.clone(),
            });
        }
        self.functions.insert(function.name.clone(), function);
        Ok(())
    }

    /// The function called `name`, if registered.
    pub fn get(&self, name: &str) -> Option<&Function> {
        self.functions.get(name)
    }

    /// All functions in name order.
    pub fn iter(&self) -> impl Iterator<Item = &Function> {
        self.functions.values()
    }
}

/// Why `name` can't be a function name for `owner`, if it can't.
fn name_problem(name: &str, owner: &Owner) -> Option<String> {
    if name == "has" {
        return Some("`has` is a built-in form".to_owned());
    }
    if owner.is_engine() {
        return name_segment_error(name)
            .map(|reason| format!("core function names are one word, which {reason}"));
    }
    let Some(module) = owner.module_name() else {
        return Some("only modules and the engine register functions".to_owned());
    };
    let Some(rest) = name
        .strip_prefix(module)
        .and_then(|rest| rest.strip_prefix('.'))
    else {
        return Some(format!(
            "functions of module `{module}` must be named `{module}.<name>`"
        ));
    };
    rest.split('.')
        .find_map(path_segment_error)
        .map(|reason| format!("the name after `{module}.` {reason}"))
}

/// Why a function could not be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistryError {
    /// A function of this name is already registered.
    Duplicate {
        /// The name.
        name: String,
        /// Who registered it first.
        owner: Owner,
    },
    /// The name is invalid, or doesn't fit its owner.
    InvalidName {
        /// The rejected name.
        name: String,
        /// Who tried to register it.
        owner: Owner,
        /// Why it was rejected.
        reason: String,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { name, owner } => {
                write!(f, "function `{name}` is already registered by {owner}")
            }
            Self::InvalidName {
                name,
                owner,
                reason,
            } => write!(f, "invalid function name `{name}` for {owner}: {reason}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// A read-only view of the state, limited to the keys a function declared.
#[derive(Debug, Clone, Copy)]
pub struct StateView<'a> {
    state: &'a State,
    reads: &'a BTreeSet<Key>,
}

impl<'a> StateView<'a> {
    pub(crate) fn new(state: &'a State, reads: &'a BTreeSet<Key>) -> Self {
        Self { state, reads }
    }

    fn check(&self, key: &Key) -> Result<(), FunctionError> {
        if self.reads.contains(key) {
            Ok(())
        } else {
            Err(FunctionError::UndeclaredRead { key: key.clone() })
        }
    }

    /// The value of `key`. Errors if the function didn't declare that it
    /// reads `key`.
    pub fn get(&self, key: &Key) -> Result<&'a Value, FunctionError> {
        self.check(key)?;
        self.state
            .get(key)
            .map_err(|e| FunctionError::State(Box::new(e)))
    }

    /// Whether the set at `key` contains `id` (see [`State::contains`]).
    /// Errors if the function didn't declare that it reads `key`.
    pub fn contains(&self, key: &Key, id: &Id) -> Result<bool, FunctionError> {
        self.check(key)?;
        self.state
            .contains(key, id)
            .map_err(|e| FunctionError::State(Box::new(e)))
    }
}

/// An error from a function's code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FunctionError {
    /// The code read a key it didn't declare.
    UndeclaredRead {
        /// The key.
        key: Key,
    },
    /// A state read failed. Boxed to keep the error small.
    State(Box<CoreError>),
    /// The function failed for its own reason.
    Failed(String),
}

impl fmt::Display for FunctionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndeclaredRead { key } => {
                write!(f, "read key `{key}`, which it doesn't declare")
            }
            Self::State(e) => write!(f, "{e}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for FunctionError {}

/// The single `time` argument of a core function.
fn time_arg(args: &[Value]) -> Result<asi_core::Time, FunctionError> {
    match args {
        [Value::Time(time)] => Ok(*time),
        _ => Err(FunctionError::Failed(
            "expected one `time` argument".to_owned(),
        )),
    }
}

fn core_day(args: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    Ok(Value::Int(time_arg(args)?.day()))
}

fn core_hour(args: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    Ok(Value::Int(time_arg(args)?.hour()))
}

fn core_minute(args: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    Ok(Value::Int(time_arg(args)?.minute()))
}
