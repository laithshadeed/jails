package net.acme.billing.infra.jdbc;

import java.sql.ResultSet;
import java.sql.SQLException;
import java.math.BigDecimal;
import net.acme.billing.domain.Invoice;

/** Row mapping this project already owns, in the layer it calls its own. */
public final class InvoiceRows {

    private InvoiceRows() {}

    public static Invoice from(ResultSet rows) throws SQLException {
        return new Invoice(
                rows.getString("reference"),
                rows.getBigDecimal("net"),
                rows.getBigDecimal("vat"));
    }
}
