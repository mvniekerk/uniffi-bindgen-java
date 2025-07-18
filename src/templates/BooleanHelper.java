package {{ config.package_name() }};

import java.nio.ByteBuffer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum FfiConverterBoolean implements FfiConverter<Boolean, Byte> {
  INSTANCE;

  @Override
  public Boolean lift(Byte value) {
    return (int) value != 0;
  }

  @Override
  public Boolean read(ByteBuffer buf) {
    return lift(buf.get());
  }

  @Override
  public Byte lower(Boolean value) {
    return value ? (byte) 1 : (byte) 0;
  }

  @Override
  public long allocationSize(Boolean value) {
    return 1L;
  }

  @Override
  public void write(Boolean value, ByteBuffer buf) {
    buf.put(lower(value));
  }
}
