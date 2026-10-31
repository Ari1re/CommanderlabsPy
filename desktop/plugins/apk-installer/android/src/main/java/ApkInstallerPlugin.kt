package com.commanderlab.apkinstaller

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

/** A FileProvider subclass of our own, so the manifest entry never clashes
 * with another library's androidx FileProvider. */
class ApkFileProvider : FileProvider()

@InvokeArg
class InstallArgs {
    lateinit var path: String
}

@TauriPlugin
class ApkInstallerPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun install(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(InstallArgs::class.java)
            val source = File(args.path)
            if (!source.exists()) {
                invoke.reject("The downloaded update is missing. Try again.")
                return
            }

            // Android 8+: the user allows this app to install apps once.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                !activity.packageManager.canRequestPackageInstalls()
            ) {
                val settings = Intent(
                    Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:" + activity.packageName)
                )
                settings.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(settings)
                val result = JSObject()
                result.put("status", "permission")
                invoke.resolve(result)
                return
            }

            // Share it from the cache folder the provider exposes.
            val dir = File(activity.cacheDir, "updates")
            dir.mkdirs()
            val apk = File(dir, "CommanderLab.apk")
            if (source.canonicalPath != apk.canonicalPath) {
                source.copyTo(apk, overwrite = true)
            }
            val uri = FileProvider.getUriForFile(activity, activity.packageName + ".apkinstaller", apk)
            val intent = Intent(Intent.ACTION_VIEW)
            intent.setDataAndType(uri, "application/vnd.android.package-archive")
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(intent)

            val result = JSObject()
            result.put("status", "started")
            invoke.resolve(result)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: ex.toString())
        }
    }
}
