package {{ config.package_name() }};

import java.lang.ref.Cleaner;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
class JavaLangRefCleaner implements UniffiCleaner {
    private final Cleaner cleaner;

    JavaLangRefCleaner() {
      this.cleaner = Cleaner.create();
    }

    @Override
    public UniffiCleaner.Cleanable register(Object value, Runnable cleanUpTask) {
        return new JavaLangRefCleanable(cleaner.register(value, cleanUpTask));
    }
}

package {{ config.package_name() }};

import java.lang.ref.Cleaner;{% if config.quarkus %}
import io.quarkus.runtime.annotations.RegisterForReflection;{%- endif %}
{% if config.quarkus %}
@RegisterForReflection{%- endif %}
class JavaLangRefCleanable implements UniffiCleaner.Cleanable {
    private final Cleaner.Cleanable cleanable;
    
    JavaLangRefCleanable(Cleaner.Cleanable cleanable) {
        this.cleanable = cleanable;
    }
    
    @Override
    public void clean() {
      cleanable.clean();
    }
}
