//! Describes items defined or visible (ie, imported) in a certain scope.
//! This is shared between modules and blocks.

use hir_expand::name::Name;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;

use crate::{per_ns::PerNs, BuiltinType, LocalImportId, MacroDefId, ModuleDefId, TraitId};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ItemScope {
    pub(crate) items: FxHashMap<Name, Resolution>,
    /// Macros visible in current module in legacy textual scope
    ///
    /// For macros invoked by an unqualified identifier like `bar!()`, `legacy_macros` will be searched in first.
    /// If it yields no result, then it turns to module scoped `macros`.
    /// It macros with name qualified with a path like `crate::foo::bar!()`, `legacy_macros` will be skipped,
    /// and only normal scoped `macros` will be searched in.
    ///
    /// Note that this automatically inherit macros defined textually before the definition of module itself.
    ///
    /// Module scoped macros will be inserted into `items` instead of here.
    // FIXME: Macro shadowing in one module is not properly handled. Non-item place macros will
    // be all resolved to the last one defined if shadowing happens.
    pub(crate) legacy_macros: FxHashMap<Name, MacroDefId>,
}

static BUILTIN_SCOPE: Lazy<FxHashMap<Name, Resolution>> = Lazy::new(|| {
    BuiltinType::ALL
        .iter()
        .map(|(name, ty)| {
            (name.clone(), Resolution { def: PerNs::types(ty.clone().into()), import: None })
        })
        .collect()
});

/// Shadow mode for builtin type which can be shadowed by module.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BuiltinShadowMode {
    // Prefer Module
    Module,
    // Prefer Other Types
    Other,
}

/// Legacy macros can only be accessed through special methods like `get_legacy_macros`.
/// Other methods will only resolve values, types and module scoped macros only.
impl ItemScope {
    pub fn entries<'a>(&'a self) -> impl Iterator<Item = (&'a Name, &'a Resolution)> + 'a {
        //FIXME: shadowing
        self.items.iter().chain(BUILTIN_SCOPE.iter())
    }

    pub fn declarations(&self) -> impl Iterator<Item = ModuleDefId> + '_ {
        self.entries()
            .filter_map(|(_name, res)| if res.import.is_none() { Some(res.def) } else { None })
            .flat_map(|per_ns| {
                per_ns.take_types().into_iter().chain(per_ns.take_values().into_iter())
            })
    }

    /// Iterate over all module scoped macros
    pub fn macros<'a>(&'a self) -> impl Iterator<Item = (&'a Name, MacroDefId)> + 'a {
        self.items
            .iter()
            .filter_map(|(name, res)| res.def.take_macros().map(|macro_| (name, macro_)))
    }

    /// Iterate over all legacy textual scoped macros visable at the end of the module
    pub fn legacy_macros<'a>(&'a self) -> impl Iterator<Item = (&'a Name, MacroDefId)> + 'a {
        self.legacy_macros.iter().map(|(name, def)| (name, *def))
    }

    /// Get a name from current module scope, legacy macros are not included
    pub fn get(&self, name: &Name, shadow: BuiltinShadowMode) -> Option<&Resolution> {
        match shadow {
            BuiltinShadowMode::Module => self.items.get(name).or_else(|| BUILTIN_SCOPE.get(name)),
            BuiltinShadowMode::Other => {
                let item = self.items.get(name);
                if let Some(res) = item {
                    if let Some(ModuleDefId::ModuleId(_)) = res.def.take_types() {
                        return BUILTIN_SCOPE.get(name).or(item);
                    }
                }

                item.or_else(|| BUILTIN_SCOPE.get(name))
            }
        }
    }

    pub fn traits<'a>(&'a self) -> impl Iterator<Item = TraitId> + 'a {
        self.items.values().filter_map(|r| match r.def.take_types() {
            Some(ModuleDefId::TraitId(t)) => Some(t),
            _ => None,
        })
    }

    pub(crate) fn get_legacy_macro(&self, name: &Name) -> Option<MacroDefId> {
        self.legacy_macros.get(name).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolution {
    /// None for unresolved
    pub def: PerNs,
    /// ident by which this is imported into local scope.
    pub import: Option<LocalImportId>,
}
