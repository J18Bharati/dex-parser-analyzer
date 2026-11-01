// Modified by Zero (2026-11-01): new fixture source (fork M5). See NOTICE.
package app.fixture;

/** The app: calls the SDKs above, and Facebook, which is only referenced (a stub). */
public final class MainActivity {
    public void onCreate() {
        com.google.firebase.analytics.FirebaseAnalytics.getInstance(this);
        io.sentry.android.core.SentryAndroid.init(this);
        com.google.android.gms.ads.MobileAds.initialize(this);
        com.facebook.FacebookSdk.sdkInitialize(this);
    }
}
