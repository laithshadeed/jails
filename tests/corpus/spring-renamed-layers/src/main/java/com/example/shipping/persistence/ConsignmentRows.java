package com.example.shipping.persistence;

import com.example.shipping.core.Consignment;
import java.sql.ResultSet;
import java.sql.SQLException;

/** Row mapping, in the layer this project calls `persistence`. */
public final class ConsignmentRows {

    private ConsignmentRows() {}

    public static Consignment from(ResultSet rows) throws SQLException {
        return new Consignment(rows.getString("reference"), rows.getInt("parcels"));
    }
}
