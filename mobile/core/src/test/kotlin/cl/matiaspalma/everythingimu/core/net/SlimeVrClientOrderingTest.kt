package cl.matiaspalma.everythingimu.core.net

import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.nio.ByteBuffer
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Regression guard for the SlimeVR-Server "Out of order packet received" crash.
 *
 * SlimeVR-Server's [UDPProtocolParser] enforces a strictly increasing packet
 * number per device and throws (logging a full stack trace and dropping the
 * datagram) when it sees a number <= the last it accepted. When the client
 * assigns sequence numbers on the caller's thread but writes the socket from
 * several different threads, the wire order diverges from the number order and
 * the server rejects a steady stream of rotation/heartbeat packets — observed
 * in the field as SlimeVR freezing after the phone streams for a while.
 *
 * This test hammers the public send API from many threads and asserts the
 * sequence numbers reach the wire strictly increasing.
 */
class SlimeVrClientOrderingTest {

    @Test
    fun concurrent_sends_emit_strictly_increasing_sequence_numbers() {
        val server = DatagramSocket(0, InetAddress.getByName("127.0.0.1"))
        server.soTimeout = 500
        val port = server.localPort
        val received = ConcurrentLinkedQueue<Long>()

        val collecting = AtomicBoolean(true)
        val collector = thread(name = "fake-server") {
            val buf = ByteArray(2048)
            val pkt = DatagramPacket(buf, buf.size)
            while (collecting.get()) {
                try {
                    server.receive(pkt)
                    if (pkt.length >= 12) {
                        received.add(ByteBuffer.wrap(pkt.data, 4, 8).long)
                    }
                } catch (_: java.net.SocketTimeoutException) {
                }
            }
        }

        val client = SlimeVrClient(mac = byteArrayOf(1, 2, 3, 4, 5, 6))
        client.connect("127.0.0.1", port)

        val threadCount = 6
        val perThread = 500
        val start = CountDownLatch(1)
        val workers = (0 until threadCount).map { i ->
            thread(name = "sender-$i") {
                start.await()
                repeat(perThread) {
                    client.sendRotation(floatArrayOf(1f, 0f, 0f, 0f))
                    client.sendAccel(floatArrayOf(0f, 0f, 9.8f))
                }
            }
        }
        start.countDown()
        workers.forEach { it.join() }

        Thread.sleep(300) // drain in-flight datagrams
        collecting.set(false)
        collector.join(2000)
        client.shutdown()
        server.close()

        var prev = Long.MIN_VALUE
        var inversions = 0
        for (seq in received) {
            if (seq <= prev) inversions++
            prev = seq
        }
        assertEquals(
            "wire sequence inverted across ${received.size} received packets",
            0,
            inversions,
        )
    }
}
