package com.example.depot.rest;

import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RestController;

/** The other. Neither is wrong; a `[layout]` table can only name one. */
@RestController
public class ReorderResource {

    @PostMapping("/reorders")
    public String raise() {
        return "raised";
    }
}
