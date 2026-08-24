package com.example.demo.web;

import com.example.demo.domain.Verification;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RestController;

/**
 * Package-private, and so is every handler on it.
 *
 * <p>Spring instantiates and calls this by reflection, so {@code public} buys
 * it nothing -- it only widens the surface other packages can compile
 * against. A controller is an entry point, not module API.
 */
@RestController
class VerifyController {

    @PostMapping("/verify")
    Verification post(@RequestBody Verification request) {
        throw new UnsupportedOperationException(
                "todo: build the Verification this route answers with");
    }
}
