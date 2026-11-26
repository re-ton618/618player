package com.player618.media;

import android.app.Activity;
import android.app.Application;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.drawable.Icon;
import android.media.MediaMetadata;
import android.media.MediaMetadataRetriever;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The system's side of playback: the media session (lock screen, headset
 * buttons, the shade's player with its progress bar), its notification, and
 * the foreground service that keeps the process playing in the background.
 *
 * <p>Rust tells it what plays ({@link #update}, {@link #clear}) from the
 * engine's thread; what the user asks of it goes back through
 * {@link #command}, which Rust registers. Everything here runs on the main
 * thread.
 */
public final class Playback {
    private static final String TAG = "618player";

    // What {@link #command} asks; Rust's `platform::media::Remote`.
    static final int PLAY = 1;
    static final int PAUSE = 2;
    static final int TOGGLE = 3;
    static final int NEXT = 4;
    static final int PREVIOUS = 5;
    static final int SEEK = 6;
    /** The app goes away: playback stops and the notification with it. */
    static final int STOP = 7;

    static final String ACTION_TOGGLE = "com.player618.media.TOGGLE";
    static final String ACTION_NEXT = "com.player618.media.NEXT";
    static final String ACTION_PREVIOUS = "com.player618.media.PREVIOUS";
    static final String ACTION_DISMISS = "com.player618.media.DISMISS";

    private static final String CHANNEL = "playback";
    static final int NOTIFICATION = 618;
    private static final String ACTIVITY = "com.longbridge.nsh.AppActivity";
    private static final int ART_SIZE = 512;

    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    private static final ExecutorService ART = Executors.newSingleThreadExecutor(r -> {
        Thread thread = new Thread(r, "618player-artwork");
        thread.setDaemon(true);
        return thread;
    });

    private static Application app;
    private static MediaSession session;
    /** What plays, as Rust last told it; null: nothing. */
    private static Now now;
    /** The service while it runs, foreground or not. */
    private static Service service;
    private static boolean foreground;
    /** The artwork of {@link #artPath}'s file, once read; null: none (yet). */
    private static String artPath;
    private static Bitmap art;
    /** The app's activity went away: what its engine still says is not shown. */
    private static boolean closed;

    private Playback() {}

    /** What Rust last said plays. */
    private static final class Now {
        final String title;
        final String artist;
        final String album;
        final String path;
        final long durationMs;
        final long positionMs;
        final boolean playing;
        /** When {@link #positionMs} was where playback was. */
        final long at;

        Now(String title, String artist, String album, String path, long durationMs, long positionMs,
                boolean playing) {
            this.title = title;
            this.artist = artist;
            this.album = album;
            this.path = path;
            this.durationMs = durationMs;
            this.positionMs = positionMs;
            this.playing = playing;
            this.at = SystemClock.elapsedRealtime();
        }
    }

    /** Rust's: the track that is loaded, where it is and whether it sounds. Any thread. */
    public static void update(Context context, String title, String artist, String album, String path,
            long durationMs, long positionMs, boolean playing) {
        Application application = (Application) context.getApplicationContext();
        Now next = new Now(title, artist, album, path, durationMs, positionMs, playing);
        MAIN.post(() -> {
            if (closed) return;
            init(application);
            show(next);
        });
    }

    /** Rust's: nothing is loaded. Any thread. */
    public static void clear(Context context) {
        MAIN.post(Playback::end);
    }

    /** Into Rust: what the user asked of playback. Registered by Rust as it first calls in. */
    private static native void command(int code, long value);

    private static boolean send(int code, long value) {
        try {
            command(code, value);
            return true;
        } catch (UnsatisfiedLinkError e) {
            // A process the system started for the service alone: no app to ask.
            return false;
        }
    }

    private static void init(Application application) {
        if (app != null) return;
        app = application;
        NotificationChannel channel = new NotificationChannel(CHANNEL, "Playback",
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription("What is playing, with its controls");
        channel.setShowBadge(false);
        manager().createNotificationChannel(channel);

        session = new MediaSession(app, "618player");
        session.setCallback(new MediaSession.Callback() {
            @Override public void onPlay() { send(PLAY, 0); }
            @Override public void onPause() { send(PAUSE, 0); }
            @Override public void onSkipToNext() { send(NEXT, 0); }
            @Override public void onSkipToPrevious() { send(PREVIOUS, 0); }
            @Override public void onStop() { send(PAUSE, 0); }
            @Override public void onSeekTo(long position) { send(SEEK, Math.max(0, position)); }
        }, MAIN);
        PendingIntent open = openApp();
        if (open != null) session.setSessionActivity(open);

        app.registerActivityLifecycleCallbacks(new Application.ActivityLifecycleCallbacks() {
            @Override public void onActivityCreated(Activity a, Bundle state) {
                if (ACTIVITY.equals(a.getClass().getName())) closed = false;
            }
            @Override public void onActivityDestroyed(Activity a) {
                // The app's activity going for good takes its engine with it:
                // nothing would be left to answer the controls.
                if (ACTIVITY.equals(a.getClass().getName()) && !a.isChangingConfigurations()) close();
            }
            @Override public void onActivityStarted(Activity a) {}
            @Override public void onActivityResumed(Activity a) {}
            @Override public void onActivityPaused(Activity a) {}
            @Override public void onActivityStopped(Activity a) {}
            @Override public void onActivitySaveInstanceState(Activity a, Bundle state) {}
        });
    }

    private static void show(Now next) {
        Now before = now;
        now = next;
        if (before == null || !same(before.path, next.path)) loadArt(next.path);
        session.setMetadata(metadata(next));
        session.setPlaybackState(state(next));
        if (!session.isActive()) session.setActive(true);
        present();
    }

    /** The notification, and the service foreground while something sounds. */
    private static void present() {
        if (now == null) return;
        Notification notification = notification(now);
        if (now.playing) {
            if (service == null) {
                try {
                    app.startForegroundService(new Intent(app, PlaybackService.class));
                } catch (RuntimeException e) {
                    Log.w(TAG, "playback service not started", e);
                    manager().notify(NOTIFICATION, notification);
                }
                // The service shows it as it starts ({@link #attach}).
                return;
            }
            promote(service, notification);
        } else {
            if (service != null && foreground) {
                // Paused: the system may stop the app, and the user may swipe the notification away.
                service.stopForeground(Service.STOP_FOREGROUND_DETACH);
                foreground = false;
            }
            manager().notify(NOTIFICATION, notification);
        }
    }

    private static void promote(Service s, Notification notification) {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                s.startForeground(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
            } else {
                s.startForeground(NOTIFICATION, notification);
            }
            foreground = true;
        } catch (RuntimeException e) {
            // Android 12+ lets an app in the background go foreground only when the user asked.
            Log.w(TAG, "playback service not foreground", e);
            manager().notify(NOTIFICATION, notification);
        }
    }

    /**
     * The service started: for the foreground ({@link #present}), it is to be
     * foreground at once; then as playback says.
     */
    static void attach(Service s, boolean forForeground) {
        service = s;
        if (now == null || app == null) {
            // Started with nothing to show: the process came back for a button
            // of a notification the last one left, which answers nothing now.
            if (forForeground && app != null) {
                // Playback ended as the service started: Android still wants it foreground
                // once before it stops.
                promote(s, new Notification.Builder(app, CHANNEL)
                        .setSmallIcon(android.R.drawable.ic_media_play)
                        .build());
                s.stopForeground(Service.STOP_FOREGROUND_REMOVE);
                foreground = false;
            }
            manager().cancel(NOTIFICATION);
            s.stopSelf();
            service = null;
            return;
        }
        if (forForeground) promote(s, notification(now));
        if (!now.playing) present();
    }

    static void detach(Service s) {
        if (service == s) {
            service = null;
            foreground = false;
        }
    }

    /** A button of the notification. */
    static void action(String action) {
        int code;
        switch (action) {
            case ACTION_TOGGLE: code = TOGGLE; break;
            case ACTION_NEXT: code = NEXT; break;
            case ACTION_PREVIOUS: code = PREVIOUS; break;
            case ACTION_DISMISS:
                // Swiped away while paused: it stays paused, and goes.
                send(PAUSE, 0);
                end();
                return;
            default: return;
        }
        if (!send(code, 0)) end();
    }

    /** The app was swiped away from the recent apps. */
    static void taskRemoved() {
        close();
    }

    /** The app's UI is gone for good: playback stops, and the session and notification go. */
    private static void close() {
        closed = true;
        send(STOP, 0);
        end();
    }

    /** Nothing plays: the notification and the service go, the session rests. */
    private static void end() {
        now = null;
        artPath = null;
        art = null;
        if (session != null) {
            session.setPlaybackState(new PlaybackState.Builder()
                    .setState(PlaybackState.STATE_STOPPED, 0, 0f)
                    .build());
            session.setActive(false);
        }
        if (service != null) {
            service.stopForeground(Service.STOP_FOREGROUND_REMOVE);
            service.stopSelf();
            service = null;
            foreground = false;
        }
        if (app != null || service != null) manager().cancel(NOTIFICATION);
    }

    private static MediaMetadata metadata(Now n) {
        MediaMetadata.Builder builder = new MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, n.title)
                .putString(MediaMetadata.METADATA_KEY_ARTIST, n.artist)
                .putString(MediaMetadata.METADATA_KEY_ALBUM, n.album);
        // The shade's progress bar is the duration's: without one, none.
        if (n.durationMs > 0) builder.putLong(MediaMetadata.METADATA_KEY_DURATION, n.durationMs);
        if (art != null && same(artPath, n.path)) builder.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, art);
        return builder.build();
    }

    private static PlaybackState state(Now n) {
        long actions = PlaybackState.ACTION_PLAY | PlaybackState.ACTION_PAUSE
                | PlaybackState.ACTION_PLAY_PAUSE | PlaybackState.ACTION_SKIP_TO_NEXT
                | PlaybackState.ACTION_SKIP_TO_PREVIOUS | PlaybackState.ACTION_STOP;
        if (n.durationMs > 0) actions |= PlaybackState.ACTION_SEEK_TO;
        // The system moves the position on from `at` at the speed given.
        return new PlaybackState.Builder()
                .setActions(actions)
                .setState(n.playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED,
                        n.positionMs, n.playing ? 1f : 0f, n.at)
                .build();
    }

    private static Notification notification(Now n) {
        Notification.Builder builder = new Notification.Builder(app, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setContentTitle(n.title)
                .setContentText(line(n.artist, n.album))
                .setVisibility(Notification.VISIBILITY_PUBLIC)
                .setCategory(Notification.CATEGORY_TRANSPORT)
                .setShowWhen(false)
                .setOnlyAlertOnce(true)
                .setOngoing(n.playing)
                .setDeleteIntent(serviceIntent(ACTION_DISMISS, 4))
                .addAction(action(android.R.drawable.ic_media_previous, "Previous", ACTION_PREVIOUS, 1))
                .addAction(n.playing
                        ? action(android.R.drawable.ic_media_pause, "Pause", ACTION_TOGGLE, 2)
                        : action(android.R.drawable.ic_media_play, "Play", ACTION_TOGGLE, 2))
                .addAction(action(android.R.drawable.ic_media_next, "Next", ACTION_NEXT, 3))
                .setStyle(new Notification.MediaStyle()
                        .setMediaSession(session.getSessionToken())
                        .setShowActionsInCompactView(0, 1, 2));
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            builder.setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE);
        }
        PendingIntent open = openApp();
        if (open != null) builder.setContentIntent(open);
        if (art != null && same(artPath, n.path)) builder.setLargeIcon(art);
        return builder.build();
    }

    private static Notification.Action action(int icon, String label, String action, int code) {
        return new Notification.Action.Builder(Icon.createWithResource(app, icon), label,
                serviceIntent(action, code)).build();
    }

    private static PendingIntent serviceIntent(String action, int code) {
        Intent intent = new Intent(app, PlaybackService.class).setAction(action);
        return PendingIntent.getService(app, code, intent,
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    private static PendingIntent openApp() {
        Intent launch = app.getPackageManager().getLaunchIntentForPackage(app.getPackageName());
        if (launch == null) return null;
        launch.setPackage(null);
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        return PendingIntent.getActivity(app, 0, launch,
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    /** Reads the artwork embedded in the file at `path` off the main thread, then shows it. */
    private static void loadArt(String path) {
        artPath = path;
        art = null;
        if (path == null) return;
        ART.execute(() -> {
            Bitmap read = readArt(path);
            MAIN.post(() -> {
                if (read == null || !same(artPath, path) || now == null) return;
                art = read;
                session.setMetadata(metadata(now));
                present();
            });
        });
    }

    private static Bitmap readArt(String path) {
        MediaMetadataRetriever retriever = new MediaMetadataRetriever();
        try {
            retriever.setDataSource(path);
            byte[] bytes = retriever.getEmbeddedPicture();
            if (bytes == null) return null;
            BitmapFactory.Options bounds = new BitmapFactory.Options();
            bounds.inJustDecodeBounds = true;
            BitmapFactory.decodeByteArray(bytes, 0, bytes.length, bounds);
            BitmapFactory.Options options = new BitmapFactory.Options();
            options.inSampleSize = 1;
            while (Math.max(bounds.outWidth, bounds.outHeight) / (options.inSampleSize * 2) >= ART_SIZE) {
                options.inSampleSize *= 2;
            }
            return BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
        } catch (RuntimeException e) {
            return null;
        } finally {
            try {
                retriever.release();
            } catch (Exception ignored) {
                // Nothing to keep.
            }
        }
    }

    private static String line(String artist, String album) {
        if (artist != null && album != null) return artist + " \u2014 " + album;
        return artist != null ? artist : album;
    }

    private static boolean same(String a, String b) {
        return a == null ? b == null : a.equals(b);
    }

    /** The app's, or the service's in a process the system started for it alone. */
    private static NotificationManager manager() {
        Context context = app != null ? app : service;
        return (NotificationManager) context.getSystemService(Context.NOTIFICATION_SERVICE);
    }
}
