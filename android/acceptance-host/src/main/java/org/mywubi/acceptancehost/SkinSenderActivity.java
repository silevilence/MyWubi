package org.mywubi.acceptancehost;

import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;

/** Sends only the bundled fixture. The receiver must resolve through its manifest and read the URI grant. */
public final class SkinSenderActivity extends Activity {
    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (state != null) { finish(); return; }
        try {
            File folder = new File(getCacheDir(), "skin-fixture");
            if (!folder.isDirectory() && !folder.mkdirs()) throw new IOException("Cannot create fixture directory");
            File skin = new File(folder, "聊天附件 竹纸.mywubi-skin");
            try (InputStream input = getAssets().open("bamboo-android.mywubi-skin");
                 FileOutputStream output = new FileOutputStream(skin)) {
                byte[] buffer = new byte[8192];
                for (int count; (count = input.read(buffer)) != -1;) output.write(buffer, 0, count);
            }
            Uri uri = Uri.parse("content://" + getPackageName() + ".skins/attachments/" + java.util.UUID.randomUUID());
            String mime = getIntent().getStringExtra("mime");
            boolean share = getIntent().getBooleanExtra("share", false);
            Intent send = share ? new Intent(Intent.ACTION_SEND).setType(mime).putExtra(Intent.EXTRA_STREAM, uri)
                                : new Intent(Intent.ACTION_VIEW).setDataAndType(uri, mime);
            if (!getIntent().getBooleanExtra("deny", false)) {
                send.setClipData(ClipData.newRawUri("skin", uri));
                send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            }
            if (getIntent().getBooleanExtra("chooser", false)) startActivity(Intent.createChooser(send, "分享皮肤文件"));
            else startActivity(send.setPackage("org.mywubi"));
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        finish();
    }
}
