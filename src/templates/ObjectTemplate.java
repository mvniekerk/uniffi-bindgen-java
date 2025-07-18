// This template implements a class for working with a Rust struct via a Pointer/Arc<T>
// to the live Rust struct on the other side of the FFI.
//
// Each instance implements core operations for working with the Rust `Arc<T>` and the
// Kotlin Pointer to work with the live Rust struct on the other side of the FFI.
//
// There's some subtlety here, because we have to be careful not to operate on a Rust
// struct after it has been dropped, and because we must expose a public API for freeing
// the Java wrapper object in lieu of reliable finalizers. The core requirements are:
//
//   * Each instance holds an opaque pointer to the underlying Rust struct.
//     Method calls need to read this pointer from the object's state and pass it in to
//     the Rust FFI.
//
//   * When an instance is no longer needed, its pointer should be passed to a
//     special destructor function provided by the Rust FFI, which will drop the
//     underlying Rust struct.
//
//   * Given an instance, calling code is expected to call the special
//     `close` method in order to free it after use, either by calling it explicitly
//     or by using a higher-level helper like `try-with-resources`. Failing to do so risks
//     leaking the underlying Rust struct.
//
//   * We can't assume that calling code will do the right thing, and must be prepared
//     to handle Java method calls executing concurrently with or even after a call to
//     `close`, and to handle multiple (possibly concurrent!) calls to `close`.
//
//   * We must never allow Rust code to operate on the underlying Rust struct after
//     the destructor has been called, and must never call the destructor more than once.
//     Doing so may trigger memory unsafety.
//
//   * To mitigate many of the risks of leaking memory and use-after-free unsafety, a `Cleaner`
//     is implemented to call the destructor when the Java object becomes unreachable.
//     This is done in a background thread. This is not a panacea, and client code should be aware that
//      1. the thread may starve if some there are objects that have poorly performing
//     `drop` methods or do significant work in their `drop` methods.
//      2. the thread is shared across the whole library. This can be tuned by using `android_cleaner = true`,
//         or `android = true` in the [`java` section of the `uniffi.toml` file, like the Kotlin one](https://mozilla.github.io/uniffi-rs/kotlin/configuration.html).
//
// If we try to implement this with mutual exclusion on access to the pointer, there is the
// possibility of a race between a method call and a concurrent call to `close`:
//
//    * Thread A starts a method call, reads the value of the pointer, but is interrupted
//      before it can pass the pointer over the FFI to Rust.
//    * Thread B calls `close` and frees the underlying Rust struct.
//    * Thread A resumes, passing the already-read pointer value to Rust and triggering
//      a use-after-free.
//
// One possible solution would be to use a `ReadWriteLock`, with each method call taking
// a read lock (and thus allowed to run concurrently) and the special `close` method
// taking a write lock (and thus blocking on live method calls). However, we aim not to
// generate methods with any hidden blocking semantics, and a `close` method that might
// block if called incorrectly seems to meet that bar.
//
// So, we achieve our goals by giving each instance an associated `AtomicLong` counter to track
// the number of in-flight method calls, and an `AtomicBoolean` flag to indicate whether `close`
// has been called. These are updated according to the following rules:
//
//    * The initial value of the counter is 1, indicating a live object with no in-flight calls.
//      The initial value for the flag is false.
//
//    * At the start of each method call, we atomically check the counter.
//      If it is 0 then the underlying Rust struct has already been destroyed and the call is aborted.
//      If it is nonzero them we atomically increment it by 1 and proceed with the method call.
//
//    * At the end of each method call, we atomically decrement and check the counter.
//      If it has reached zero then we destroy the underlying Rust struct.
//
//    * When `close` is called, we atomically flip the flag from false to true.
//      If the flag was already true we silently fail.
//      Otherwise we atomically decrement and check the counter.
//      If it has reached zero then we destroy the underlying Rust struct.
//
// Astute readers may observe that this all sounds very similar to the way that Rust's `Arc<T>` works,
// and indeed it is, with the addition of a flag to guard against multiple calls to `close`.
//
// The overall effect is that the underlying Rust struct is destroyed only when `close` has been
// called *and* all in-flight method calls have completed, avoiding violating any of the expectations
// of the underlying Rust code.
//
// This makes a cleaner a better alternative to _not_ calling `close()` as
// and when the object is finished with, but the abstraction is not perfect: if the Rust object's `drop`
// method is slow, and/or there are many objects to cleanup, and it's on a low end Android device, then the cleaner
// thread may be starved, and the app will leak memory.
//
// In this case, `close`ing manually may be a better solution.
//
// The cleaner can live side by side with the manual calling of `close`. In the order of responsiveness, uniffi objects
// with Rust peers are reclaimed:
//
// 1. By calling the `close` method of the object, which calls `rustObject.free()`. If that doesn't happen:
// 2. When the object becomes unreachable, AND the Cleaner thread gets to call `rustObject.free()`. If the thread is starved then:
// 3. The memory is reclaimed when the process terminates.
//
// [1] https://stackoverflow.com/questions/24376768/can-java-finalize-an-object-when-it-is-still-in-scope/24380219
//

{%- if self.include_once_check("interface-support") %}
  {%- include "ObjectCleanerHelper.java" %}
{%- endif %}

{%- let obj = ci.get_object_definition(name).unwrap() %}
{%- let (interface_name, impl_class_name) = obj|object_names(ci) %}
{%- let methods = obj.methods() %}
{%- let interface_docstring = obj.docstring() %}
{%- let is_error = ci.is_name_used_as_error(name) %}
{%- let ffi_converter_name = obj|ffi_converter_name %}

{%- include "Interface.java" %}

package {{ config.package_name() }};

import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Function;
import java.util.function.Consumer;
import com.sun.jna.Pointer;
import java.util.concurrent.CompletableFuture;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}

{%- call java::docstring(obj, 0) %}
{% if (is_error) %}{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public class {{ impl_class_name }} extends Exception implements AutoCloseable, {{ interface_name }} {
{% else -%}{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public class {{ impl_class_name }} implements AutoCloseable, {{ interface_name }} {
{%- endif %}
  protected Pointer pointer;
  protected UniffiCleaner.Cleanable cleanable;

  private AtomicBoolean wasDestroyed = new AtomicBoolean(false);
  private AtomicLong callCounter = new AtomicLong(1);

  public {{ impl_class_name }}(Pointer pointer) {
    this.pointer = pointer;
    this.cleanable = UniffiLib.CLEANER.register(this, new UniffiCleanAction(pointer));
  }

  /**
   * This constructor can be used to instantiate a fake object. Only used for tests. Any
   * attempt to actually use an object constructed this way will fail as there is no
   * connected Rust object.
   */
  public {{ impl_class_name }}(NoPointer noPointer) {
    this.pointer = null;
    this.cleanable = UniffiLib.CLEANER.register(this, new UniffiCleanAction(pointer));
  }

  {% match obj.primary_constructor() %}
  {%- when Some(cons) %}
  {%-     if cons.is_async() %}
  // Note no constructor generated for this object as it is async.
  {%-     else %}
  {%- call java::docstring(cons, 4) %}
  public {{ impl_class_name }}({% call java::arg_list(cons, true) -%}) {% match cons.throws_type() %}{% when Some(throwable) %}throws {{ throwable|type_name(ci, config) }}{% else %}{% endmatch %}{
    this((Pointer){%- call java::to_ffi_call(cons) -%});
  }
  {%-     endif %}
  {%- when None %}
  {%- endmatch %}

  @Override
  public synchronized void close() {
    // Only allow a single call to this method.
    // TODO(uniffi): maybe we should log a warning if called more than once?
    if (this.wasDestroyed.compareAndSet(false, true)) {
      // This decrement always matches the initial count of 1 given at creation time.
      if (this.callCounter.decrementAndGet() == 0L) {
        cleanable.clean();
      }
    }
  }

  public <R> R callWithPointer(Function<Pointer, R> block) {
    // Check and increment the call counter, to keep the object alive.
    // This needs a compare-and-set retry loop in case of concurrent updates.
    long c;
    do {
      c = this.callCounter.get();
      if (c == 0L) {
        throw new IllegalStateException("{{ impl_class_name }} object has already been destroyed");
      }
      if (c == Long.MAX_VALUE) {
        throw new IllegalStateException("{{ impl_class_name }} call counter would overflow");
      }
    } while (! this.callCounter.compareAndSet(c, c + 1L));
    // Now we can safely do the method call without the pointer being freed concurrently.
    try {
      return block.apply(this.uniffiClonePointer());
    } finally {
      // This decrement always matches the increment we performed above.
      if (this.callCounter.decrementAndGet() == 0L) {
          cleanable.clean();
      }
    }
  }

  public void callWithPointer(Consumer<Pointer> block) {
    callWithPointer((Pointer p) -> {
      block.accept(p);
      return (Void)null;
    });
  }

  private class UniffiCleanAction implements Runnable {
    private final Pointer pointer;

    public UniffiCleanAction(Pointer pointer) {
      this.pointer = pointer;
    }

    @Override
    public void run() {
      if (pointer != null) {
        UniffiHelpers.uniffiRustCall(status -> {
          UniffiLib.getInstance().{{ obj.ffi_object_free().name() }}(pointer, status);
          return null;
        });
      }
    }
  }

  Pointer uniffiClonePointer() {
    return UniffiHelpers.uniffiRustCall(status -> {
      if (pointer == null) {
        throw new NullPointerException();
      }
      return UniffiLib.getInstance().{{ obj.ffi_object_clone().name() }}(pointer, status);
    });
  }

  {% for meth in obj.methods() -%}
  {%- call java::func_decl("public", "Override", meth, 4) %}
  {% endfor %}

  {%- for tm in obj.uniffi_traits() %}
  {%-     match tm %}
  {%         when UniffiTrait::Display { fmt } %}
  @Override
  public String toString() {
      return {{ fmt.return_type().unwrap()|lift_fn(config, ci) }}({% call java::to_ffi_call(fmt) %});
  }
  {%         when UniffiTrait::Eq { eq, ne } %}
  {# only equals used #}
  @Override
  public Boolean equals(Object other) {
      if (this === other) {
        return true;
      }
      if (!(other instanceof {{ impl_class_name}})) {
        return false;
      }
      return {{ eq.return_type().unwrap()|lift_fn(config, ci) }}({% call java::to_ffi_call(eq) %});
  }
  {%         when UniffiTrait::Hash { hash } %}
  @Override
  public Integer hashCode() {
      return {{ hash.return_type().unwrap()|lift_fn(config, ci) }}({%- call java::to_ffi_call(hash) %}).toInt();
  }
  {%-         else %}
  {%-     endmatch %}
  {%- endfor %}

  {% if !obj.alternate_constructors().is_empty() -%}
  {% for cons in obj.alternate_constructors() -%}
  {% call java::func_decl("public static", "", cons, 4) %}
  {% endfor %}
  {% endif %}
}

{% if is_error %}
package {{ config.package_name() }};{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public class {{ impl_class_name }}ErrorHandler implements UniffiRustCallStatusErrorHandler<{{ impl_class_name }}> {
    @Override
    public {{ impl_class_name }} lift(RustBuffer.ByValue error_buf) {
        // Due to some mismatches in the ffi converter mechanisms, errors are a RustBuffer.
        var bb = error_buf.asByteBuffer();
        if (bb == null) {
            throw new InternalException("?");
        }
        return {{ ffi_converter_instance }}.read(bb);
    }
}
{% endif %}

{%- if obj.has_callback_interface() %}
{%- let vtable = obj.vtable().expect("trait interface should have a vtable") %}
{%- let vtable_methods = obj.vtable_methods() %}
{%- let ffi_init_callback = obj.ffi_init_callback() %}
{% include "CallbackInterfaceImpl.java" %}
{%- endif %}

package {{ config.package_name() }};

import java.nio.ByteBuffer;
import com.sun.jna.Pointer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum {{ ffi_converter_name }} implements FfiConverter<{{ type_name }}, Pointer> {
    INSTANCE;

    {%- if obj.has_callback_interface() %}
    public final UniffiHandleMap<{{ type_name }}> handleMap = new UniffiHandleMap<>();
    {%- endif %}

    @Override
    public Pointer lower({{ type_name }} value) {
        {%- if obj.has_callback_interface() %}
        return new Pointer(handleMap.insert(value));
        {%- else %}
        return value.uniffiClonePointer();
        {%- endif %}
    }

    @Override
    public {{ type_name }} lift(Pointer value) {
        return new {{ impl_class_name }}(value);
    }

    @Override
    public {{ type_name }} read(ByteBuffer buf) {
        // The Rust code always writes pointers as 8 bytes, and will
        // fail to compile if they don't fit.
        return lift(new Pointer(buf.getLong()));
    }

    @Override
    public long allocationSize({{ type_name }} value) {
      return 8L;
    }

    @Override
    public void write({{ type_name }} value, ByteBuffer buf) {
        // The Rust code always expects pointers written as 8 bytes,
        // and will fail to compile if they don't fit.
        buf.putLong(Pointer.nativeValue(lower(value)));
    }
}
