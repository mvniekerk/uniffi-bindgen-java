package {{ config.package_name() }};

import java.nio.ByteBuffer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum FfiConverterLong implements FfiConverter<Long, Long> {
    INSTANCE;

    @Override
    public Long lift(Long value) {
        return value;
    }

    @Override
    public Long read(ByteBuffer buf) {
        return buf.getLong();
    }

    @Override
    public Long lower(Long value) {
        return value;
    }

    @Override
    public long allocationSize(Long value) {
        return 8L;
    }

    @Override
    public void write(Long value, ByteBuffer buf) {
        buf.putLong(value);
    }
}

