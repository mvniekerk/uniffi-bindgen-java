package uniffi.quarkus.service;

import io.smallrye.mutiny.Uni;
import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;
import jakarta.ws.rs.Produces;
import jakarta.ws.rs.QueryParam;
import jakarta.ws.rs.core.MediaType;
import uniffi.quarkus.SayTrue;

@Path("/")
public class SayTrueResource {

    @GET
    @Produces(MediaType.APPLICATION_JSON)
    public Uni<Boolean> sayYes(@QueryParam("trueOrNot") Boolean trueOrNot) {
        return Uni.createFrom().item(SayTrue.sayTrueOrNot(Boolean.TRUE.equals(trueOrNot)));
    }
}
