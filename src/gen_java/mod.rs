use anyhow::{Context, Result};
use askama::Template;
use core::fmt::Debug;
use heck::{ToLowerCamelCase, ToShoutySnakeCase, ToUpperCamelCase};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use uniffi_bindgen::{backend::Literal, interface::*};

mod callback_interface;
mod compounds;
mod custom;
mod enum_;
mod miscellany;
mod object;
mod primitives;
mod record;
mod variant;

pub fn potentially_add_external_package(
    config: &Config,
    ci: &ComponentInterface,
    type_name: &str,
    display_name: String,
) -> String {
    match ci.get_type(type_name) {
        Some(typ) => {
            if ci.is_external(&typ) {
                format!(
                    "{}.{}",
                    config.external_type_package_name(typ.module_path().unwrap(), &display_name),
                    display_name
                )
            } else {
                display_name
            }
        }
        None => display_name,
    }
}

trait CodeType: Debug {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, ci: &ComponentInterface, config: &Config) -> String;

    /// A representation of this type label that can be used as part of another
    /// identifier. e.g. `read_foo()`, or `FooInternals`.
    ///
    /// This is especially useful when creating specialized objects or methods to deal
    /// with this type only.
    fn canonical_name(&self) -> String;

    fn literal(&self, _literal: &Literal, ci: &ComponentInterface, config: &Config) -> String {
        unimplemented!("Unimplemented for {}", self.type_label(ci, config))
    }

    /// Instance of the FfiConverter
    ///
    /// This is the object that contains the lower, write, lift, and read methods for this type.
    /// Depending on the binding this will either be a singleton or a class with static methods.
    ///
    /// This is the newer way of handling these methods and replaces the lower, write, lift, and
    /// read CodeType methods.
    fn ffi_converter_name(&self) -> String {
        format!("FfiConverter{}", self.canonical_name())
    }

    /// Name of the FfiConverter
    ///
    /// This is the object that contains the lower, write, lift, and read methods for this type.
    /// Depending on the binding this will either be a singleton or a class with static methods.
    ///
    /// This is the newer way of handling these methods and replaces the lower, write, lift, and
    /// read CodeType methods.
    fn ffi_converter_instance(&self, _config: &Config, _ci: &ComponentInterface) -> String {
        format!("{}.INSTANCE", self.ffi_converter_name())
    }

    /// A list of imports that are needed if this type is in use.
    /// Classes are imported exactly once.
    fn imports(&self) -> Option<Vec<String>> {
        None
    }

    /// Function to run at startup
    fn initialization_fn(&self) -> Option<String> {
        None
    }
}

// taken from https://docs.oracle.com/javase/specs/ section 3.9
static KEYWORDS: Lazy<HashSet<String>> = Lazy::new(|| {
    let kwlist = vec![
        "abstract",
        "continue",
        "for",
        "new",
        "switch",
        "assert",
        "default",
        "if",
        "package",
        "synchronized",
        "boolean",
        "do",
        "goto",
        "private",
        "this",
        "break",
        "double",
        "implements",
        "protected",
        "throw",
        "byte",
        "else",
        "import",
        "public",
        "throws",
        "case",
        "enum",
        "instanceof",
        "return",
        "transient",
        "catch",
        "extends",
        "int",
        "short",
        "try",
        "char",
        "final",
        "interface",
        "static",
        "void",
        "class",
        "finally",
        "long",
        "strictfp",
        "volatile",
        "const",
        "float",
        "native",
        "super",
        "while",
        "_",
    ];
    HashSet::from_iter(kwlist.into_iter().map(|s| s.to_string()))
});

// config options to customize the generated Java.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    pub(super) package_name: Option<String>,
    pub(super) cdylib_name: Option<String>,
    generate_immutable_records: Option<bool>,
    #[serde(default)]
    custom_types: HashMap<String, CustomTypeConfig>,
    #[serde(default)]
    pub(super) external_packages: HashMap<String, String>,
    #[serde(default)]
    android: bool,
    #[serde(default)]
    android_cleaner: Option<bool>,
    #[serde(default)]
    quarkus: bool
}

impl Config {
    pub(crate) fn android_cleaner(&self) -> bool {
        self.android_cleaner.unwrap_or(self.android)
    }
}

impl Config {
    pub fn package_name(&self) -> String {
        if let Some(package_name) = &self.package_name {
            package_name.clone()
        } else {
            "uniffi".into()
        }
    }

    pub fn cdylib_name(&self) -> String {
        if let Some(cdylib_name) = &self.cdylib_name {
            cdylib_name.clone()
        } else {
            "uniffi".into()
        }
    }

    /// Whether to generate immutable records (`record` instead of `class`)
    pub fn generate_immutable_records(&self) -> bool {
        self.generate_immutable_records.unwrap_or(false)
    }

    // Get the package name for an external type
    fn external_type_package_name(&self, module_path: &str, namespace: &str) -> String {
        // config overrides are keyed by the crate name, default fallback is the namespace.
        let crate_name = module_path.split("::").next().unwrap();
        match self.external_packages.get(crate_name) {
            Some(name) => name.clone(),
            // unreachable in library mode - all deps are in our config with correct namespace.
            None => format!("uniffi.{namespace}"),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomTypeConfig {
    imports: Option<Vec<String>>,
    type_name: Option<String>,
    into_custom: String, // backcompat alias for lift
    lift: String,
    from_custom: String, // backcompat alias for lower
    lower: String,
}

// functions replace literal "{}" in strings with a specified value.
impl CustomTypeConfig {
    fn lift(&self, name: &str) -> String {
        let converter = if self.lift.is_empty() {
            &self.into_custom
        } else {
            &self.lift
        };
        converter.replace("{}", name)
    }
    fn lower(&self, name: &str) -> String {
        let converter = if self.lower.is_empty() {
            &self.from_custom
        } else {
            &self.lower
        };
        converter.replace("{}", name)
    }
}

// Generate Java bindings for the given ComponentInterface, as a string.
pub fn generate_bindings(config: &Config, ci: &ComponentInterface) -> Result<String> {
    JavaWrapper::new(config.clone(), ci)
        .render()
        .context("failed to render java bindings")
}

#[derive(Template)]
#[template(syntax = "java", escape = "none", path = "wrapper.java")]
pub struct JavaWrapper<'a> {
    config: Config,
    ci: &'a ComponentInterface,
    type_helper_code: String,
}

impl<'a> JavaWrapper<'a> {
    pub fn new(config: Config, ci: &'a ComponentInterface) -> Self {
        let type_renderer = TypeRenderer::new(&config, ci);
        let type_helper_code = type_renderer.render().unwrap();
        Self {
            config,
            ci,
            type_helper_code,
        }
    }

    pub fn initialization_fns(&self) -> Vec<String> {
        self.ci
            .iter_local_types()
            .map(|t| JavaCodeOracle.find(t))
            .filter_map(|ct| ct.initialization_fn())
            .collect()
    }
}

/// Renders Java helper code for all types
///
/// This template is a bit different than others in that it stores internal state from the render
/// process.  Make sure to only call `render()` once.
#[derive(Template)]
#[template(syntax = "java", escape = "none", path = "Types.java")]
pub struct TypeRenderer<'a> {
    config: &'a Config,
    ci: &'a ComponentInterface,
    // Track included modules for the `include_once()` macro
    include_once_names: RefCell<HashSet<String>>,
}

impl<'a> TypeRenderer<'a> {
    fn new(config: &'a Config, ci: &'a ComponentInterface) -> Self {
        Self {
            config,
            ci,
            include_once_names: RefCell::new(HashSet::new()),
        }
    }

    // The following methods are used by the `Types.java` macros.

    // Helper for the including a template, but only once.
    //
    // The first time this is called with a name it will return true, indicating that we should
    // include the template.  Subsequent calls will return false.
    fn include_once_check(&self, name: &str) -> bool {
        self.include_once_names
            .borrow_mut()
            .insert(name.to_string())
    }
}

fn fixup_keyword(name: String) -> String {
    if KEYWORDS.contains(&name) {
        format!("_{name}")
    } else {
        name
    }
}

#[derive(Clone)]
pub struct JavaCodeOracle;

impl JavaCodeOracle {
    fn find(&self, type_: &Type) -> Box<dyn CodeType> {
        type_.clone().as_type().as_codetype()
    }

    /// Get the idiomatic Java rendering of a class name (for enums, records, errors, etc).
    fn class_name(&self, ci: &ComponentInterface, nm: &str) -> String {
        let name = nm.to_string().to_upper_camel_case();
        // fixup errors.
        fixup_keyword(
            ci.is_name_used_as_error(nm)
                .then(|| self.convert_error_suffix(&name))
                .unwrap_or(name),
        )
    }

    fn convert_error_suffix(&self, nm: &str) -> String {
        match nm.strip_suffix("Error") {
            None => nm.to_string(),
            Some(stripped) => format!("{stripped}Exception"),
        }
    }

    /// Get the idiomatic Java rendering of a function name.
    fn fn_name(&self, nm: &str) -> String {
        fixup_keyword(nm.to_string().to_lower_camel_case())
    }

    /// Get the idiomatic Java rendering of a variable name.
    pub fn var_name(&self, nm: &str) -> String {
        fixup_keyword(self.var_name_raw(nm))
    }

    /// `var_name` without the reserved word alteration.  Useful for using in `@Structure.FieldOrder`.
    pub fn var_name_raw(&self, nm: &str) -> String {
        nm.to_string().to_lower_camel_case()
    }

    /// Get the idiomatic setter name for a variable.
    pub fn setter(&self, nm: &str) -> String {
        format!("set{}", fixup_keyword(nm.to_string().to_upper_camel_case()))
    }

    /// Get the idiomatic Java rendering of an individual enum variant.
    fn enum_variant_name(&self, nm: &str) -> String {
        nm.to_string().to_shouty_snake_case()
    }

    /// Get the idiomatic Java rendering of an FFI callback function name
    fn ffi_callback_name(&self, nm: &str) -> String {
        format!("Uniffi{}", nm.to_upper_camel_case())
    }

    /// Get the idiomatic Java rendering of an FFI struct name
    fn ffi_struct_name(&self, nm: &str) -> String {
        format!("Uniffi{}", nm.to_upper_camel_case())
    }

    fn ffi_type_label_by_value(
        &self,
        ffi_type: &FfiType,
        prefer_primitive: bool,
        config: &Config,
        ci: &ComponentInterface,
    ) -> String {
        match ffi_type {
            FfiType::RustBuffer(_) => {
                format!("{}.ByValue", self.ffi_type_label(ffi_type, config, ci))
            }
            FfiType::Struct(name) => format!("{}.UniffiByValue", self.ffi_struct_name(name)),
            _ if prefer_primitive => self.ffi_type_primitive(ffi_type, config, ci),
            _ => self.ffi_type_label(ffi_type, config, ci),
        }
    }

    /// FFI type name to use inside structs
    ///
    /// The main requirement here is that all types must have default values or else the struct
    /// won't work in some JNA contexts.
    fn ffi_type_label_for_ffi_struct(
        &self,
        ffi_type: &FfiType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> String {
        match ffi_type {
            // Make callbacks function pointers nullable. This matches the semantics of a C
            // function pointer better and allows for `null` as a default value.
            // Everything is nullable in Java by default.
            FfiType::Callback(name) => self.ffi_callback_name(name).to_string(),
            _ => self.ffi_type_label_by_value(ffi_type, true, config, ci),
        }
    }

    /// Default values for FFI
    ///
    /// This is used to:
    ///   - Set a default return value for error results
    ///   - Set a default for structs, which JNA sometimes requires
    fn ffi_default_value(&self, ffi_type: &FfiType) -> String {
        match ffi_type {
            FfiType::UInt8 | FfiType::Int8 => "(byte)0".to_owned(),
            FfiType::UInt16 | FfiType::Int16 => "(short)0".to_owned(),
            FfiType::UInt32 | FfiType::Int32 => "0".to_owned(),
            FfiType::UInt64 | FfiType::Int64 => "0L".to_owned(),
            FfiType::Float32 => "0.0f".to_owned(),
            FfiType::Float64 => "0.0".to_owned(),
            FfiType::RustArcPtr(_) => "Pointer.NULL".to_owned(),
            FfiType::RustBuffer(_) => "new RustBuffer.ByValue()".to_owned(),
            FfiType::Callback(_) => "null".to_owned(),
            FfiType::RustCallStatus => "new UniffiRustCallStatus.ByValue()".to_owned(),
            _ => unimplemented!("ffi_default_value: {ffi_type:?}"),
        }
    }

    fn ffi_type_label_by_reference(
        &self,
        ffi_type: &FfiType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> String {
        match ffi_type {
            FfiType::Int32 | FfiType::UInt32 => "IntByReference".to_string(),
            FfiType::Int8
            | FfiType::UInt8
            | FfiType::Int16
            | FfiType::UInt16
            | FfiType::Int64
            | FfiType::UInt64
            | FfiType::Float32
            | FfiType::Float64 => {
                format!("{}ByReference", self.ffi_type_label(ffi_type, config, ci))
            }
            FfiType::RustArcPtr(_) => "PointerByReference".to_owned(),
            // JNA structs default to ByReference
            FfiType::RustBuffer(_) | FfiType::Struct(_) => {
                self.ffi_type_label(ffi_type, config, ci)
            }
            _ => panic!("{ffi_type:?} by reference is not implemented"),
        }
    }

    fn ffi_type_label(
        &self,
        ffi_type: &FfiType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> String {
        match ffi_type {
            // Note that unsigned values in Java don't have true native support. Signed primitives
            // can contain unsigned values and there are methods like `Integer.compareUnsigned`
            // that respect the unsigned value, but knowledge outside the type system is required.
            // TODO(java): improve callers knowledge of what contains an unsigned value
            FfiType::Int8 | FfiType::UInt8 => "Byte".to_string(),
            FfiType::Int16 | FfiType::UInt16 => "Short".to_string(),
            FfiType::Int32 | FfiType::UInt32 => "Integer".to_string(),
            FfiType::Int64 | FfiType::UInt64 => "Long".to_string(),
            FfiType::Float32 => "Float".to_string(),
            FfiType::Float64 => "Double".to_string(),
            FfiType::Handle => "Long".to_string(),
            FfiType::RustArcPtr(_) => "Pointer".to_string(),
            FfiType::RustBuffer(maybe_external) => match maybe_external {
                Some(external_meta) if external_meta.module_path != ci.crate_name() => {
                    format!(
                        "{}.RustBuffer",
                        config.external_type_package_name(
                            &external_meta.module_path,
                            &external_meta.name
                        )
                    )
                }
                _ => "RustBuffer".to_string(),
            },
            FfiType::RustCallStatus => "UniffiRustCallStatus.ByValue".to_string(),
            FfiType::ForeignBytes => "ForeignBytes.ByValue".to_string(),
            FfiType::Callback(name) => self.ffi_callback_name(name),
            FfiType::Struct(name) => self.ffi_struct_name(name),
            FfiType::Reference(inner) | FfiType::MutReference(inner) => {
                self.ffi_type_label_by_reference(inner, config, ci)
            }
            FfiType::VoidPointer => "Pointer".to_string(),
        }
    }

    /// Generate primitive types where possible. Useful where we don't need or can't have boxed versions (ie structs).
    fn ffi_type_primitive(
        &self,
        ffi_type: &FfiType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> String {
        match ffi_type {
            // Note that unsigned integers in Java are currently experimental, but java.nio.ByteBuffer does not
            // support them yet. Thus, we use the signed variants to represent both signed and unsigned
            // types from the component API.
            FfiType::Int8 | FfiType::UInt8 => "byte".to_string(),
            FfiType::Int16 | FfiType::UInt16 => "short".to_string(),
            FfiType::Int32 | FfiType::UInt32 => "int".to_string(),
            FfiType::Int64 | FfiType::UInt64 => "long".to_string(),
            FfiType::Float32 => "float".to_string(),
            FfiType::Float64 => "double".to_string(),
            FfiType::Handle => "long".to_string(),
            FfiType::RustArcPtr(_) => "Pointer".to_string(),
            FfiType::RustBuffer(maybe_external) => match maybe_external {
                Some(external_meta) => {
                    format!(
                        "{}.RustBuffer",
                        config.external_type_package_name(
                            &external_meta.module_path,
                            &external_meta.name
                        )
                    )
                }
                None => "RustBuffer".to_string(),
            },
            FfiType::RustCallStatus => "UniffiRustCallStatus.ByValue".to_string(),
            FfiType::ForeignBytes => "ForeignBytes.ByValue".to_string(),
            FfiType::Callback(name) => self.ffi_callback_name(name),
            FfiType::Struct(name) => self.ffi_struct_name(name),
            FfiType::Reference(inner) | FfiType::MutReference(inner) => {
                self.ffi_type_label_by_reference(inner, config, ci)
            }
            FfiType::VoidPointer => "Pointer".to_string(),
        }
    }

    /// Get the name of the interface and class name for an object.
    ///
    /// If we support callback interfaces, the interface name is the object name, and the class name is derived from that.
    /// Otherwise, the class name is the object name and the interface name is derived from that.
    ///
    /// This split determines what types `FfiConverter.lower()` inputs.  If we support callback
    /// interfaces, `lower` must lower anything that implements the interface.  If not, then lower
    /// only lowers the concrete class.
    fn object_names(&self, ci: &ComponentInterface, obj: &Object) -> (String, String) {
        let class_name = self.class_name(ci, obj.name());
        if obj.has_callback_interface() {
            let impl_name = format!("{class_name}Impl");
            (class_name, impl_name)
        } else {
            (format!("{class_name}Interface"), class_name)
        }
    }
}

trait AsCodeType {
    fn as_codetype(&self) -> Box<dyn CodeType>;
}

// Workaround for the possibility of upstream additions of AsType breaking compilation
// Downside to this is new types need to be manually added
impl AsCodeType for Type {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        // Map `Type` instances to a `Box<dyn CodeType>` for that type.
        //
        // There is a companion match in `templates/Types.java` which performs a similar function for the
        // template code.
        //
        //   - When adding additional types here, make sure to also add a match arm to the `Types.java` template.
        //   - To keep things manageable, let's try to limit ourselves to these 2 mega-matches
        match self.as_type() {
            Type::UInt8 | Type::Int8 => Box::new(primitives::Int8CodeType),
            Type::UInt16 | Type::Int16 => Box::new(primitives::Int16CodeType),
            Type::UInt32 | Type::Int32 => Box::new(primitives::Int32CodeType),
            Type::UInt64 | Type::Int64 => Box::new(primitives::Int64CodeType),
            Type::Float32 => Box::new(primitives::Float32CodeType),
            Type::Float64 => Box::new(primitives::Float64CodeType),
            Type::Boolean => Box::new(primitives::BooleanCodeType),
            Type::String => Box::new(primitives::StringCodeType),
            Type::Bytes => Box::new(primitives::BytesCodeType),

            Type::Timestamp => Box::new(miscellany::TimestampCodeType),
            Type::Duration => Box::new(miscellany::DurationCodeType),

            Type::Enum { name, .. } => Box::new(enum_::EnumCodeType::new(name.clone())),
            Type::Object { name, imp, .. } => {
                Box::new(object::ObjectCodeType::new(name.clone(), imp))
            }
            Type::Record { name, .. } => Box::new(record::RecordCodeType::new(name.clone())),
            Type::CallbackInterface { name, .. } => Box::new(
                callback_interface::CallbackInterfaceCodeType::new(name.clone()),
            ),
            Type::Optional { inner_type } => {
                Box::new(compounds::OptionalCodeType::new((*inner_type).clone()))
            }
            Type::Sequence { inner_type } => {
                Box::new(compounds::SequenceCodeType::new((*inner_type).clone()))
            }
            Type::Map {
                key_type,
                value_type,
            } => Box::new(compounds::MapCodeType::new(
                (*key_type).clone(),
                (*value_type).clone(),
            )),
            Type::Custom { name, .. } => Box::new(custom::CustomCodeType::new(name.clone())),
        }
    }
}
impl AsCodeType for &'_ Type {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        (*self).as_codetype()
    }
}
impl AsCodeType for &&'_ Type {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        (**self).as_codetype()
    }
}
impl AsCodeType for &'_ Field {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}
impl AsCodeType for &'_ uniffi_bindgen::interface::Enum {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}
impl AsCodeType for &'_ uniffi_bindgen::interface::Object {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}
impl AsCodeType for &'_ Box<uniffi_meta::Type> {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}
impl AsCodeType for &'_ Argument {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}
impl AsCodeType for &'_ uniffi_bindgen::interface::Record {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}
impl AsCodeType for &'_ uniffi_bindgen::interface::CallbackInterface {
    fn as_codetype(&self) -> Box<dyn CodeType> {
        self.as_type().as_codetype()
    }
}

// A work around for #2392 - we can't handle functions with external errors.
fn can_render_callable(callable: &dyn Callable, ci: &ComponentInterface) -> bool {
    // can't handle external errors.
    callable
        .throws_type()
        .map(|t| !ci.is_external(t))
        .unwrap_or(true)
}

mod filters {
    use super::*;
    pub use uniffi_bindgen::backend::filters::*;
    use uniffi_bindgen::interface::ffi::ExternalFfiMetadata;
    use uniffi_meta::LiteralMetadata;

    pub(super) fn type_name(
        as_ct: &impl AsCodeType,
        ci: &ComponentInterface,
        config: &Config,
    ) -> Result<String, askama::Error> {
        Ok(as_ct.as_codetype().type_label(ci, config))
    }

    pub(super) fn canonical_name(as_ct: &impl AsCodeType) -> Result<String, askama::Error> {
        Ok(as_ct.as_codetype().canonical_name())
    }

    pub(super) fn ffi_converter_instance(
        as_ct: &impl AsCodeType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(as_ct.as_codetype().ffi_converter_instance(config, ci))
    }

    pub(super) fn ffi_converter_name(as_ct: &impl AsCodeType) -> Result<String, askama::Error> {
        Ok(as_ct.as_codetype().ffi_converter_name())
    }

    pub(super) fn lower_fn(
        as_ct: &impl AsCodeType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(format!(
            "{}.lower",
            as_ct.as_codetype().ffi_converter_instance(config, ci)
        ))
    }

    pub(super) fn allocation_size_fn(
        as_ct: &impl AsCodeType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(format!(
            "{}.allocationSize",
            as_ct.as_codetype().ffi_converter_instance(config, ci)
        ))
    }

    pub(super) fn write_fn(
        as_ct: &impl AsCodeType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(format!(
            "{}.write",
            as_ct.as_codetype().ffi_converter_instance(config, ci)
        ))
    }

    pub(super) fn lift_fn(
        as_ct: &impl AsCodeType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(format!(
            "{}.lift",
            as_ct.as_codetype().ffi_converter_instance(config, ci)
        ))
    }

    pub(super) fn read_fn(
        as_ct: &impl AsCodeType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(format!(
            "{}.read",
            as_ct.as_codetype().ffi_converter_instance(config, ci)
        ))
    }

    // Get the idiomatic Java rendering of an integer.
    fn int_literal(t: &Option<Type>, base10: String) -> Result<String, askama::Error> {
        if let Some(t) = t {
            match t {
                Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64 => Ok(base10),
                Type::UInt8 | Type::UInt16 | Type::UInt32 | Type::UInt64 => Ok(base10 + "u"),
                _ => Err(to_askama_error("Only ints are supported.")),
            }
        } else {
            Err(to_askama_error("Enum hasn't defined a repr"))
        }
    }

    // Get the idiomatic Java rendering of an individual enum variant's discriminant
    pub fn variant_discr_literal(e: &Enum, index: &usize) -> Result<String, askama::Error> {
        let literal = e.variant_discr(*index).expect("invalid index");
        match literal {
            // Java doesn't convert between signed and unsigned by default
            // so we'll need to make sure we define the type as appropriately
            LiteralMetadata::UInt(v, _, _) => int_literal(e.variant_discr_type(), v.to_string()),
            LiteralMetadata::Int(v, _, _) => int_literal(e.variant_discr_type(), v.to_string()),
            _ => Err(to_askama_error("Only ints are supported.")),
        }
    }

    pub fn ffi_type_name_by_value(
        type_: &FfiType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.ffi_type_label_by_value(type_, false, config, ci))
    }

    pub fn ffi_type_name_for_ffi_struct(
        type_: &FfiType,
        config: &Config,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.ffi_type_label_for_ffi_struct(type_, config, ci))
    }

    pub fn ffi_default_value(type_: FfiType) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.ffi_default_value(&type_))
    }

    /// Get the idiomatic Java rendering of a class name.
    pub fn class_name<S: AsRef<str>>(
        nm: S,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.class_name(ci, nm.as_ref()))
    }

    /// Get the idiomatic Java rendering of a function name.
    pub fn fn_name<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.fn_name(nm.as_ref()))
    }

    /// Get the idiomatic Java rendering of a variable name.
    pub fn var_name<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.var_name(nm.as_ref()))
    }

    /// Get the idiomatic Java rendering of a variable name, without altering reserved words.
    pub fn var_name_raw<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.var_name_raw(nm.as_ref()))
    }

    /// Get the idiomatic Java setter method name.
    pub fn setter<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.setter(nm.as_ref()))
    }

    /// Get a String representing the name used for an individual enum variant.
    pub fn variant_name(v: &Variant) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.enum_variant_name(v.name()))
    }

    pub fn error_variant_name(v: &Variant) -> Result<String, askama::Error> {
        let name = v.name().to_string().to_upper_camel_case();
        Ok(JavaCodeOracle.convert_error_suffix(&name))
    }

    /// Get the idiomatic Java rendering of an FFI callback function name
    pub fn ffi_callback_name<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.ffi_callback_name(nm.as_ref()))
    }

    /// Get the idiomatic Java rendering of an FFI struct name
    pub fn ffi_struct_name<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(JavaCodeOracle.ffi_struct_name(nm.as_ref()))
    }

    pub fn object_names(
        obj: &Object,
        ci: &ComponentInterface,
    ) -> Result<(String, String), askama::Error> {
        Ok(JavaCodeOracle.object_names(ci, obj))
    }

    pub fn async_inner_return_type(
        callable: impl Callable,
        ci: &ComponentInterface,
        config: &Config,
    ) -> Result<String, askama::Error> {
        callable
            .return_type()
            .map_or(Ok("Void".to_string()), |t| type_name(t, ci, config))
    }

    pub fn async_return_type(
        callable: impl Callable,
        ci: &ComponentInterface,
        config: &Config,
    ) -> Result<String, askama::Error> {
        let is_async = callable.is_async();
        let inner_type = async_inner_return_type(callable, ci, config)?;
        if is_async {
            Ok(format!("CompletableFuture<{inner_type}>"))
        } else {
            Ok(inner_type)
        }
    }

    pub fn async_poll(
        callable: impl Callable,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        let ffi_func = callable.ffi_rust_future_poll(ci);
        Ok(format!(
            "(future, callback, continuation) -> UniffiLib.getInstance().{ffi_func}(future, callback, continuation)"
        ))
    }

    pub fn async_complete(
        callable: impl Callable,
        ci: &ComponentInterface,
        config: &Config,
    ) -> Result<String, askama::Error> {
        let ffi_func = callable.ffi_rust_future_complete(ci);
        let call = format!("UniffiLib.getInstance().{ffi_func}(future, continuation)");
        let call = match callable.return_type() {
            Some(return_type) if ci.is_external(return_type) => {
                let ffi_type = FfiType::from(return_type);
                match ffi_type {
                    FfiType::RustBuffer(Some(ExternalFfiMetadata { name, module_path })) => {
                        // Need to convert the RustBuffer from our package to the RustBuffer of the external package
                        let rust_buffer = format!(
                            "{}.RustBuffer",
                            config.external_type_package_name(&module_path, &name)
                        );
                        format!(
                            "(future, continuation) -> {{
                    var result = {call};
                    return {rust_buffer}.create(result.capacity, result.len, result.data);
                }}"
                        )
                    }
                    _ => call,
                }
            }
            _ => format!("(future, continuation) -> {call}"),
        };
        Ok(call)
    }

    pub fn async_free(
        callable: impl Callable,
        ci: &ComponentInterface,
    ) -> Result<String, askama::Error> {
        let ffi_func = callable.ffi_rust_future_free(ci);
        Ok(format!("(future) -> UniffiLib.getInstance().{ffi_func}(future)"))
    }

    /// Remove the "`" chars we put around function/variable names
    ///
    /// These are used to avoid name clashes with java identifiers, but sometimes you want to
    /// render the name unquoted.  One example is the message property for errors where we want to
    /// display the name for the user.
    pub fn unquote<S: AsRef<str>>(nm: S) -> Result<String, askama::Error> {
        Ok(nm.as_ref().trim_matches('`').to_string())
    }

    /// Get the idiomatic Java rendering of docstring
    pub fn docstring<S: AsRef<str>>(docstring: S, spaces: &i32) -> Result<String, askama::Error> {
        let middle = textwrap::indent(&textwrap::dedent(docstring.as_ref()), " * ");
        let wrapped = format!("/**\n{middle}\n */");

        let spaces = usize::try_from(*spaces).unwrap_or_default();
        Ok(textwrap::indent(&wrapped, &" ".repeat(spaces)))
    }
}
