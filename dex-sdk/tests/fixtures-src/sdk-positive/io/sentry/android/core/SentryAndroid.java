// Modified by Zero (2026-11-01): new fixture source (fork M5). See NOTICE.
package io.sentry.android.core;

/** Defined, in a sub-package: still matches sentry. */
public final class SentryAndroid {
    public static void init(Object context) { io.sentry.Sentry.init("dsn"); }
}
