package {{ config.package_name() }};

import java.nio.ByteBuffer;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
public enum FfiConverterByteArray implements FfiConverterRustBuffer<byte[]>{
  INSTANCE;

    @Override
    public byte[] read(ByteBuffer buf) {
        int len = buf.getInt();
        byte[] byteArr = new byte[len];
        buf.get(byteArr);
        return byteArr;
    }
  
    @Override
    public long allocationSize(byte[] value) {
        return 4L + (long)value.length;
    }

    @Override
    public void write(byte[] value, ByteBuffer buf) {
        buf.putInt(value.length);
        buf.put(value);
    }
}
