{%- import "macros.java" as java %}

package {{ config.package_name() }};

import java.util.List;
import java.util.Map;
import java.util.stream.Stream;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForProxy;{%- endif %}
{% if config.quarkus %}
@RegisterForProxy{%- endif %}
public interface AutoCloseableHelper {
    static void close(Object... args) {
        Stream
            .of(args)
            .forEach(obj -> {
                // this is all to avoid the problem reported in uniffi-rs#2467
                if (obj instanceof AutoCloseable) {
                    try {
                        ((AutoCloseable) obj).close();
                    } catch (Exception e) {
                        throw new RuntimeException(e);
                    }
                }
                if (obj instanceof List<?>) {
                    for (int i = 0; i < ((List) obj).size(); i++) {
                        Object element = ((List) obj).get(i);
                        if (element instanceof AutoCloseable) {
                            try {
                                ((AutoCloseable) element).close();
                            } catch (Exception e) {
                                throw new RuntimeException(e);
                            }
                        }
                    }
                }
                if (obj instanceof Map<?, ?>) {
                    for (var value : ((Map) obj).values()) {
                        if (value instanceof AutoCloseable) {
                            try {
                                ((AutoCloseable) value).close();
                            } catch (Exception e) {
                                throw new RuntimeException(e);
                            }
                        }
                    }
                }
                if (obj instanceof Iterable<?>) {
                    for (var value : ((Iterable) obj)) {
                        if (value instanceof AutoCloseable) {
                            try {
                                ((AutoCloseable) value).close();
                            } catch (Exception e) {
                                throw new RuntimeException(e);
                            }
                        }
                    }
                }
            });
    }
}
package {{ config.package_name() }};
{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public class NoPointer {
    // Private constructor to prevent instantiation
    private NoPointer() {}

    // Static final instance of the class so it can be used in tests
    public static final NoPointer INSTANCE = new NoPointer();
}

{%- for type_ in ci.iter_local_types() %}
{%- let type_name = type_|type_name(ci, config) %}
{%- let ffi_converter_name = type_|ffi_converter_name %}
{%- let ffi_converter_instance = type_|ffi_converter_instance(config, ci) %}
{%- let canonical_type_name = type_|canonical_name %}
{%- let contains_object_references = ci.item_contains_object_references(type_) %}

{#
 # Map `Type` instances to an include statement for that type.
 #
 # There is a companion match in `JavaCodeOracle::create_code_type()` which performs a similar function for the
 # Rust code.
 #
 #   - When adding additional types here, make sure to also add a match arm to that function.
 #   - To keep things manageable, let's try to limit ourselves to these 2 mega-matches
 #}
{%- match type_ %}

{%- when Type::Boolean %}
{%- include "BooleanHelper.java" %}

{%- when Type::Bytes %}
{%- include "ByteArrayHelper.java" %}

{%- when Type::CallbackInterface { module_path, name } %}
{% include "CallbackInterfaceTemplate.java" %}

{%- when Type::Custom { module_path, name, builtin } %}
{%- if !ci.is_external(type_) %}
{% include "CustomTypeTemplate.java" %}
{%- endif %}

{%- when Type::Duration %}
{% include "DurationHelper.java" %}

{%- when Type::Enum { name, module_path } %}
{%- let e = ci.get_enum_definition(name).unwrap() %}
{%- if !ci.is_name_used_as_error(name) %}
{% include "EnumTemplate.java" %}
{%- else %}
{% include "ErrorTemplate.java" %}
{%- endif -%}

{%- when Type::Int64 or Type::UInt64 %}
{%- include "Int64Helper.java" %}

{%- when Type::Int8 or Type::UInt8 %}
{%- include "Int8Helper.java" %}

{%- when Type::Int16 or Type::UInt16 %}
{%- include "Int16Helper.java" %}

{%- when Type::Int32 or Type::UInt32 %}
{%- include "Int32Helper.java" %}

{%- when Type::Float32 %}
{%- include "Float32Helper.java" %}

{%- when Type::Float64 %}
{%- include "Float64Helper.java" %}

{%- when Type::Map { key_type, value_type } %}
{% include "MapTemplate.java" %}

{%- when Type::Optional { inner_type } %}
{% include "OptionalTemplate.java" %}

{%- when Type::Object { module_path, name, imp } %}
{% include "ObjectTemplate.java" %}

{%- when Type::Record { name, module_path } %}
{% include "RecordTemplate.java" %}

{%- when Type::Sequence { inner_type } %}
{% include "SequenceTemplate.java" %}

{%- when Type::String %}
{%- include "StringHelper.java" %}

{%- when Type::Timestamp %}
{% include "TimestampHelper.java" %}

{%- else %}
{%- endmatch %}
{%- endfor %}
