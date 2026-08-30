package com.example.depot.controllers;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

/** One of two directories this project serves HTTP from. */
@RestController
public class StockController {

    @GetMapping("/stock")
    public int onHand() {
        return 0;
    }
}
