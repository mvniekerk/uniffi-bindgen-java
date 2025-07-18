package {{ config.package_name() }};

import java.nio.ByteBuffer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum FfiConverterShort implements FfiConverter<Short, Short>{
  INSTANCE;

    @Override
    public Short lift(Short value) {
        return value;
    }

    @Override
    public Short read(ByteBuffer buf) {
        return buf.getShort();
    }

    @Override
    public Short lower(Short value) {
        return value;
    }

    @Override
    public long allocationSize(Short value) {
        return 2L;
    }

    @Override
    public void write(Short value, ByteBuffer buf) {
        buf.putShort(value);
    }
}
