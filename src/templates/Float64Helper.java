package {{ config.package_name() }};

import java.nio.ByteBuffer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum FfiConverterDouble implements FfiConverter<Double, Double>{
  INSTANCE;

    @Override
    public Double lift(Double value) {
        return value;
    }

    @Override
    public Double read(ByteBuffer buf) {
        return buf.getDouble();
    }

    @Override
    public Double lower(Double value) {
        return value;
    }

    @Override
    public long allocationSize(Double value) {
        return 8L;
    }

    @Override
    public void write(Double value, ByteBuffer buf) {
        buf.putDouble(value);
    }
}
