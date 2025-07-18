package {{ config.package_name() }};

import java.nio.ByteBuffer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum FfiConverterInteger implements FfiConverter<Integer, Integer>{
  INSTANCE;

    @Override
    public Integer lift(Integer value) {
        return value;
    }

    @Override
    public Integer read(ByteBuffer buf) {
        return buf.getInt();
    }

    @Override
    public Integer lower(Integer value) {
        return value;
    }

    @Override
    public long allocationSize(Integer value) {
        return 4L;
    }

    @Override
    public void write(Integer value, ByteBuffer buf) {
        buf.putInt(value);
    }
}
