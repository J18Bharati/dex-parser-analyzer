// Modified by Jatin Bharati (2026-11-01): new fixture source (fork M5). See NOTICE.
package app.fixture;

/** The app: only references Facebook (a stub), which defines nothing here. */
public final class MainActivity {
    public void onCreate() {
        com.facebook.FacebookSdk.sdkInitialize(this);
        new com.google.firebase.analyticsx.Analytics();
        new io.sentryx.Tracker();
        new com.example.firebase_like.Tracker();
    }
}
