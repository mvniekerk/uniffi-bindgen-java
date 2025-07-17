package uniffi.quarkus.service;

import com.sun.jna.Callback;
import com.sun.jna.CallbackReference;
import com.sun.jna.Function;
import com.sun.jna.Library;
import com.sun.jna.NativeLibrary;
import com.sun.jna.NativeMapped;
import com.sun.jna.Pointer;
import com.sun.jna.Structure;
import com.sun.jna.Native;
import com.sun.jna.WString;
import com.sun.jna.platform.FileUtils;
import com.sun.jna.platform.KeyboardUtils;
import com.sun.jna.platform.WindowUtils;
import com.sun.jna.ptr.PointerByReference;
import io.quarkus.runtime.annotations.RegisterForProxy;
import io.quarkus.runtime.annotations.RegisterForReflection;
import uniffi.quarkus.RustBuffer;
import uniffi.quarkus.UniffiRustCallStatus;

@RegisterForReflection(
        targets = {
                Native.class,
                Native.ffi_callback.class,
                Structure.class,
                Structure.ByValue.class,
                Pointer.class,
                PointerByReference.class,
                RustBuffer.class,
                RustBuffer.ByValue.class,
                RustBuffer.ByReference.class,
                UniffiRustCallStatus.class,
                UniffiRustCallStatus.ByValue.class,
                Function.class,
                CallbackReference.class,
                RustBuffer.class,
                NativeLibrary.class,
                WString.class,
                NativeMapped.class,
                Object.class,
        },
        registerFullHierarchy = true
)
@RegisterForProxy(
        targets = {
                Library.class,
                Callback.class,
                Native.ffi_callback.class,
        }
)
public class Reflection {
}
