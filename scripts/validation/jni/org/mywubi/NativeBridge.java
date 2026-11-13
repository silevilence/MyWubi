package org.mywubi;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.Calendar;
import java.util.GregorianCalendar;
import java.util.Locale;
import java.util.TimeZone;

/** Headless desktop-JVM JNI harness. It is not Android ART/device acceptance. */
public final class NativeBridge {
    public static native int version();
    public static native byte[] exchange(byte[] request);

    public static void main(String[] args) throws Exception {
        System.setOut(new PrintStream(System.out, true, StandardCharsets.UTF_8));
        System.load(args[0]);
        if (version() != 1) throw new IllegalStateException("Unexpected protocol");
        try (var lines = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            for (String line; (line = lines.readLine()) != null;) {
                if (line.startsWith("@zone=")) {
                    TimeZone.setDefault(TimeZone.getTimeZone(line.substring(6)));
                    System.out.println("true");
                } else if (line.startsWith("@locale=")) {
                    Locale.setDefault(Locale.forLanguageTag(line.substring(8)));
                    System.out.println("true");
                } else if (line.equals("@time")) {
                    var now = new GregorianCalendar();
                    System.out.printf(Locale.ROOT, "{\"year\":%d,\"month\":%d,\"day\":%d,\"hour\":%d}%n",
                        now.get(Calendar.YEAR), now.get(Calendar.MONTH) + 1,
                        now.get(Calendar.DAY_OF_MONTH), now.get(Calendar.HOUR_OF_DAY));
                } else {
                    System.out.println(new String(exchange(line.getBytes(StandardCharsets.UTF_8)), StandardCharsets.UTF_8));
                }
                System.out.flush();
            }
        }
    }
}
