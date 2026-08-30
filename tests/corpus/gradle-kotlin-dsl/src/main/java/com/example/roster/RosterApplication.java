package com.example.roster;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

/** At the package root, so the base package is not a guess between siblings. */
@SpringBootApplication
public class RosterApplication {

    public static void main(String[] args) {
        SpringApplication.run(RosterApplication.class, args);
    }
}
