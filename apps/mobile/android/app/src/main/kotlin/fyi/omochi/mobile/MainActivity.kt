package fyi.omochi.mobile

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import android.content.Context
import android.net.wifi.WifiManager

class MainActivity : FlutterActivity() {
    private external fun initSharedHub(context: Context)
    private var multicast: WifiManager.MulticastLock? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        System.loadLibrary("omochi_native_bridge")
        initSharedHub(applicationContext)
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "omochi/network")
            .setMethodCallHandler { call, result ->
                try {
                    when (call.method) {
                        "acquireMulticast" -> {
                            if (multicast == null) {
                                val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
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
    override fun onDestroy() {
        while (multicast?.isHeld == true) multicast?.release()
        super.onDestroy()
    }
}
