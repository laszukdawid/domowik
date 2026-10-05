package invalid.domowik_spike.domowik_spike_app

import android.os.Bundle
import android.system.Os
import android.system.OsConstants
import android.util.Log
import io.flutter.embedding.android.FlutterActivity
import java.io.File

/**
 * The spike's Android runner. It solves two problems, both of which exist only
 * because an Android application is not a process you start from a shell.
 *
 * FIRST, HOW A PROBE IS REQUESTED. On macOS and Linux the two headless probes
 * are switched on with an environment variable. An Android activity has no
 * environment to switch on: it is started by the system from an
 * [android.content.Intent], and nothing a caller puts in that intent reaches
 * `Platform.environment` on the Dart side. Flutter's supported channel for this
 * is [getDartEntrypointArgs], whose result is handed to the Dart entry point, so
 * `main` receives it as its `List<String>`.
 *
 * SECOND, HOW A PROBE IS HEARD. The probes report by writing to stdout. Android
 * points an app's file descriptors 1 and 2 at `/dev/null`, and the
 * `log.redirect-stdio` system property does not help: it replaces Java's
 * `System.out` and `System.err`, while the Dart VM writes to the descriptors
 * directly. This was observed, not assumed -- the first run of the self-test
 * here started, ran and exited with the process dying cleanly and not one line
 * of its output surviving. [redirectStdioToProbeLog] repoints those two
 * descriptors at a file the harness reads afterwards, which is what makes a
 * probe's verdict readable by whoever started it.
 *
 * Both behaviours are inert unless a probe was requested, so an ordinary launch
 * of this app is exactly the launch a person gets.
 */
class MainActivity : FlutterActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // BEFORE super.onCreate, which is what starts the Flutter engine and
        // with it the Dart entry point. A redirection installed afterwards
        // would race the first thing the probe prints.
        if (!requestedProbe().isNullOrBlank()) {
            redirectStdioToProbeLog()
        }
        giveDartAWritableTempDirectory()
        publishNativeProvenance()
        super.onCreate(savedInstanceState)
    }

    override fun getDartEntrypointArgs(): List<String> {
        val probe = requestedProbe()
        if (probe.isNullOrBlank()) {
            return emptyList()
        }
        return listOf("$PROBE_ARGUMENT_PREFIX$probe")
    }

    private fun requestedProbe(): String? = intent?.getStringExtra(PROBE_EXTRA)

    /**
     * Publishes the two paths only the platform side can report, so the
     * provenance check has something real to compare `dladdr`'s answer against.
     *
     * The shared check asks whether the loaded image came from inside THIS
     * application. On macOS and Linux a root can be derived from
     * [Platform.resolvedExecutable]; on Android it cannot, because the
     * executable is the zygote (`/system/bin/app_process64`) and says nothing
     * about which app is running. The install directory contains the package
     * name and an installation id that changes on every install, so it is not
     * derivable at all -- only `ApplicationInfo` knows it.
     *
     * Both are published rather than one, because which is correct depends on
     * how the library is packaged, and that is a property of the build rather
     * than of the platform:
     *
     *  - `sourceDir` is this APK. With `extractNativeLibs=false`, which is what
     *    this build produces, the loader maps the library straight out of the
     *    APK and `dladdr` reports `<sourceDir>!/lib/<abi>/<soname>`, so
     *    comparing against `sourceDir` proves the library came out of THIS
     *    APK file and not merely out of this directory.
     *  - `nativeLibraryDir` is where the library would be if it HAD been
     *    extracted. It is published so the comparison can be made correctly if
     *    the packaging mode ever changes, rather than silently becoming a
     *    check that passes for the wrong reason.
     */
    private fun publishNativeProvenance() {
        try {
            Os.setenv(APK_VARIABLE, applicationInfo.sourceDir, true)
            Os.setenv(NATIVE_LIB_DIR_VARIABLE, applicationInfo.nativeLibraryDir, true)
            // NOT printed from here. Two attempts to report these from Kotlin
            // produced nothing on a real run -- println because Android
            // discards Java's stdout, and a direct write to descriptor 1
            // because whatever FileDescriptor.out refers to in an app process
            // is not the descriptor dup2 repointed. The Dart side prints what
            // it actually RECEIVES instead, which is the stronger evidence: it
            // reports the value the consumer got, not the value the producer
            // believes it sent.
        } catch (error: Throwable) {
            Log.e(LOG_TAG, "could not publish native provenance", error)
        }
    }

    /**
     * Points `TMPDIR` at this app's cache directory.
     *
     * `Directory.systemTemp` resolves to `TMPDIR` and falls back to `/tmp`.
     * There is no writable `/tmp` on Android, so the shared self-test's
     * save-and-reopen round trip -- which writes a `.domo` file, reopens it and
     * compares derived geometry -- died there with an uncaught exception and
     * took the final verdict with it. That was observed: the probe's output
     * stopped after the panic-containment check and the process exited without
     * a banner.
     *
     * Setting the variable here rather than special-casing Android in the probe
     * keeps ONE self-test across all three platforms. `Os.setenv` writes the
     * real process environment, which is what the Dart VM reads through
     * `getenv`, and it must happen before the engine starts for the same reason
     * the stdio redirection must.
     *
     * This is not probe-only. An editor that cannot write a temporary file is
     * broken on Android whoever is asking, so the ordinary launch gets it too.
     */
    private fun giveDartAWritableTempDirectory() {
        try {
            Os.setenv("TMPDIR", cacheDir.absolutePath, true)
            // User preferences must survive cache cleanup and not rely on HOME,
            // which an Android application need not have.
            Os.setenv("XDG_CONFIG_HOME", filesDir.absolutePath, true)
        } catch (error: Throwable) {
            Log.e(LOG_TAG, "could not set TMPDIR; temporary files will fail", error)
        }
    }

    /**
     * Points file descriptors 1 and 2 at a file this app owns and the harness
     * can read.
     *
     * Android points an app's descriptors 1 and 2 at `/dev/null`, and the
     * `log.redirect-stdio` system property does not help: it replaces Java's
     * `System.out` and `System.err`, while the Dart VM writes to the
     * descriptors directly. Observed, not assumed -- the first run of the
     * self-test here started, ran and exited with the process dying cleanly and
     * not one line of its output surviving.
     *
     * A FILE, and not a pipe drained into logcat, and that choice was also
     * forced by evidence. The pipe version worked for most of a run and then
     * lost the last few lines every time -- including the verdict banner, which
     * is the only line that matters. A probe ends with `exit()`, which tears the
     * process down immediately, and the thread draining the pipe lives INSIDE
     * that process, so whatever was still in the pipe buffer died with its
     * reader. Writing to a file has no reader to lose: the bytes are in the page
     * cache the moment the write returns, and survive the process that wrote
     * them. A truncated probe log is the worst possible failure here, because it
     * looks exactly like a probe that hung.
     *
     * stderr is included deliberately. A panic contained by the Rust boundary
     * still prints Rust's default panic message to stderr (finding F3), and on a
     * platform where stderr goes nowhere that evidence would simply vanish.
     */
    private fun redirectStdioToProbeLog() {
        try {
            val directory = getExternalFilesDir(null)
                ?: throw IllegalStateException("no app-specific external files directory")
            val log = File(directory, PROBE_LOG_NAME)
            val fd = Os.open(
                log.absolutePath,
                OsConstants.O_WRONLY or OsConstants.O_CREAT or OsConstants.O_TRUNC,
                LOG_FILE_MODE,
            )
            Os.dup2(fd, 1)
            Os.dup2(fd, 2)
            // The original descriptor is redundant once duplicated onto 1 and
            // 2, and leaving it open leaks it for the life of the process.
            Os.close(fd)
        } catch (error: Throwable) {
            // Reported rather than swallowed. Without the redirection a probe
            // runs and says nothing, which a harness cannot tell apart from a
            // probe that failed to start.
            Log.e(LOG_TAG, "stdio redirection failed; probe output will be lost", error)
        }
    }

    companion object {
        /** `adb shell am start ... -e domowik_probe selftest` */
        const val PROBE_EXTRA: String = "domowik_probe"

        /** Matched by `probe_request.dart` on the Dart side. */
        const val PROBE_ARGUMENT_PREFIX: String = "--domowik-probe="

        /** This APK, published for the shared provenance check. */
        const val APK_VARIABLE: String = "DOMOWIK_ANDROID_APK"

        /** Where the library would live if it were extracted rather than mapped. */
        const val NATIVE_LIB_DIR_VARIABLE: String = "DOMOWIK_ANDROID_NATIVE_LIB_DIR"

        /** Where a failed redirection is reported, since stdout is gone. */
        const val LOG_TAG: String = "DomowikSpike"

        /**
         * The probe log, under `getExternalFilesDir`, which is app-specific
         * storage needing no permission and readable over adb at
         * `/sdcard/Android/data/<applicationId>/files/`.
         */
        const val PROBE_LOG_NAME: String = "domowik-probe.log"

        /** `rw-------`: this file is the app's, and nothing else needs it. */
        const val LOG_FILE_MODE: Int = 384
    }
}
