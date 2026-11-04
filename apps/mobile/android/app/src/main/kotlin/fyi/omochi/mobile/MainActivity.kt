package fyi.omochi.mobile

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import android.content.Context
import android.net.wifi.WifiManager

class MainActivity : FlutterActivity() {
    private external fun initSharedHub(context: Context)
    private companion object {
        var engine: FlutterEngine? = null
        var multicast: WifiManager.MulticastLock? = null
    }

    // Keep the Dart owner alive with its process lease when Android destroys only
    // the Activity. A replacement host reattaches to the same Hub and isolate.
    override fun provideFlutterEngine(context: Context): FlutterEngine =
        engine ?: FlutterEngine(
            context.applicationContext,
            flutterShellArgs.toArray(),
        ).also { engine = it }

    override fun shouldDestroyEngineWithHost(): Boolean = false

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        val appContext = applicationContext
        System.loadLibrary("omochi_native_bridge")
        initSharedHub(appContext)
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "omochi/identity-lease")
            .setMethodCallHandler { call, result ->
                try {
                    val value = call.arguments as? String
                        ?: throw IllegalArgumentException("Invalid lease argument")
                    when (call.method) {
                        "acquire" -> result.success(IdentityLeases.acquire(appContext.filesDir, value))
                        "release" -> {
                            IdentityLeases.release(value)
                            result.success(null)
                        }
                        else -> result.notImplemented()
                    }
                } catch (error: Exception) {
                    result.error("identity-lease", error.message, null)
                }
            }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "omochi/network")
            .setMethodCallHandler { call, result ->
                try {
                    when (call.method) {
                        "acquireMulticast" -> {
                            if (multicast == null) {
                                val wifi = appContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
                                multicast = wifi.createMulticastLock("omochi-gateway-discovery")
                            }
                            multicast?.acquire()
                            result.success(null)
                        }
                        "releaseMulticast" -> {
                            if (multicast?.isHeld == true) multicast?.release()
                            result.success(null)
                        }
                        else -> result.notImplemented()
                    }
                } catch (error: Exception) {
                    result.error("multicast", error.message, null)
                }
            }
    }
}
