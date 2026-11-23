package org.mywubi.acceptancehost;

import android.app.Activity;
import android.os.Bundle;
import android.webkit.WebView;

/** Independent editor using the system WebView and only the bundled acceptance form. */
public final class WebEditorActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        WebView web = new WebView(this);
        web.getSettings().setJavaScriptEnabled(true);
        web.getSettings().setBlockNetworkLoads(true);
        setContentView(web);
        int field = getIntent().getIntExtra("field", 0);
        long run = getIntent().getLongExtra("run", 0L);
        web.loadUrl("file:///android_asset/host.html?field=" + field + "&run=" + run);
    }
}
