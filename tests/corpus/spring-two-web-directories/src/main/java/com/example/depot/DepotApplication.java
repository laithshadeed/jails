package com.example.depot;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

/** At the package root, so the base package is not a guess between siblings. */
@SpringBootApplication
public class DepotApplication {

    public static void main(String[] args) {
        SpringApplication.run(DepotApplication.class, args);
    }
}
