package com.example.shipping.rest;

import com.example.shipping.core.Consignment;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RestController;

/** The web layer, which this project calls `rest`. */
@RestController
public class ConsignmentEndpoint {

    @GetMapping("/consignments/{reference}")
    public Consignment byReference(@PathVariable String reference) {
        return new Consignment(reference, 1);
    }
}
