package com.player618.media;

import android.app.Service;
import android.content.Intent;
import android.os.IBinder;

/**
 * Keeps the process playing while the app is in the background: foreground
 * while something sounds ({@link Playback}), and the target of the
 * notification's buttons.
 */
public final class PlaybackService extends Service {
    private boolean attached;

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent == null ? null : intent.getAction();
        if (!attached) {
            // Started for the foreground (`startForegroundService`) or by a button:
            // either way it is to show the notification at once.
            attached = true;
            Playback.attach(this, action == null);
        }
        if (action != null) Playback.action(action);
        return START_NOT_STICKY;
    }

    /** Swiped away from the recent apps: playback stops with the app. */
    @Override
    public void onTaskRemoved(Intent root) {
        Playback.taskRemoved();
        super.onTaskRemoved(root);
    }

    @Override
    public void onDestroy() {
        Playback.detach(this);
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
