package io.github.kiliankoe.publictransport.smoke

import android.content.Context
import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okio.Buffer
import publictransport.BundledNetwork
import publictransport.Client

object Smoke {
    /**
     * Asks a server in this app for departures, which answers with a
     * recorded answer, and says how many came back and with which line the
     * first one.
     */
    @JvmStatic
    fun departures(context: Context): String =
        MockWebServer().use { server ->
            val answer = context.assets.open("vvo-departures.json").use { it.readBytes() }
            server.enqueue(MockResponse.Builder().body(Buffer().write(answer)).build())
            server.start()
            // A bundled network, so that the app also reads the data in the Rust library.
            val network = BundledNetwork.VVO_EFA.network().copy(endpoint = server.url("/std3").toString())
            val departures = runBlocking { Client("publictransport-smoke/1.0").departures(network, "de:14612:28") }
            "${departures.size} ${departures.first().line.name}"
        }
}
