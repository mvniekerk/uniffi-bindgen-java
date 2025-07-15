package uniffi.quarkus.service;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;
import com.sun.jna.ptr.PointerByReference;
import io.quarkus.runtime.annotations.RegisterForReflection;
import uniffi.quarkus.RustBuffer;

@RegisterForReflection(
        targets = {
                Structure.class,
                Pointer.class,
                PointerByReference.class
        },
        registerFullHierarchy = true
)
public class Reflection {
}
