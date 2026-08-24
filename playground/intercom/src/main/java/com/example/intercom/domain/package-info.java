/**
 * Every reference type in this package is non-null unless it is explicitly
 * annotated {@code @Nullable}.
 *
 * <p>This is a package-level opt-in because that is the only level JSpecify
 * offers: without it the package is "unspecified nullness" and a nullness
 * checker has nothing to check.
 */
@NullMarked
package com.example.intercom.domain;

import org.jspecify.annotations.NullMarked;
