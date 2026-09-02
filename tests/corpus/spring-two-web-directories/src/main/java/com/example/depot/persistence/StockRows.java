package com.example.depot.persistence;

import java.sql.ResultSet;
import java.sql.SQLException;

/** Unambiguous, and must still be recorded: one undecidable layer does not
 * make the others undecidable. */
public final class StockRows {

    private StockRows() {}

    public static int onHand(ResultSet rows) throws SQLException {
        return rows.getInt("on_hand");
    }
}
