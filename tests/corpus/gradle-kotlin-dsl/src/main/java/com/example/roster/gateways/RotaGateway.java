package com.example.roster.gateways;

import com.example.roster.model.Shift;
import java.util.List;

/** Outbound calls, in the directory this project calls `gateways`. */
public interface RotaGateway {

    List<Shift> published(String team);
}
