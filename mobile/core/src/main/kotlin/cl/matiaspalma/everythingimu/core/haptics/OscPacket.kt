package cl.matiaspalma.everythingimu.core.haptics

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Minimal OSC 1.0 parser. Only the parts VRChat actually emits on UDP 9001:
 *   - top-level `#bundle` packets with an `i64` timetag + sequence of size-prefixed sub-packets
 *   - individual messages: address (OSC-string), type tag (',' prefix), then args
 *
 * Supported argument types: `i` (int32), `f` (float32), `T`/`F` (bool true/false).
 * Everything else is silently skipped. Returning a list keeps the listener
 * loop allocation-light enough for the typical avatar update rate.
 */
data class OscMessage(val address: String, val args: List<Any>) {
    fun firstFloat(): Float? = args.firstOrNull { it is Float } as? Float
    fun firstBool(): Boolean? = args.firstOrNull { it is Boolean } as? Boolean
    fun firstInt(): Int? = args.firstOrNull { it is Int } as? Int

    /** Best-effort 0..1 intensity: float kept as-is, bool→1f/0f, int>0 → 1f. */
    fun intensity(): Float = when (val v = args.firstOrNull()) {
        is Float -> v.coerceIn(0f, 1f)
        is Boolean -> if (v) 1f else 0f
        is Int -> if (v > 0) 1f else 0f
        else -> 0f
    }
}

object Osc {

    private const val BUNDLE_HEADER = "#bundle"

    fun parse(data: ByteArray, length: Int): List<OscMessage> {
        if (length <= 0) return emptyList()
        val out = ArrayList<OscMessage>(4)
        val buf = ByteBuffer.wrap(data, 0, length).order(ByteOrder.BIG_ENDIAN)
        parseInto(buf, length, out)
        return out
    }

    private fun parseInto(buf: ByteBuffer, length: Int, out: MutableList<OscMessage>) {
        val start = buf.position()
        if (length - start < 4) return
        // Peek for bundle marker
        if (data(buf, start) == '#'.code.toByte()) {
            val header = readOscString(buf) ?: return
            if (header == BUNDLE_HEADER) {
                if (buf.remaining() < 8) return
                buf.position(buf.position() + 8) // skip timetag
                while (buf.remaining() >= 4) {
                    val size = buf.int
                    if (size <= 0 || size > buf.remaining()) return
                    val subStart = buf.position()
                    val sub = buf.slice().order(ByteOrder.BIG_ENDIAN)
                    sub.limit(size)
                    parseInto(sub, size, out)
                    buf.position(subStart + size)
                }
                return
            } else {
                // Treat as a regular message whose address started with '#'
                val rest = parseMessageBody(buf, header)
                if (rest != null) out.add(rest)
                return
            }
        }
        val msg = parseMessage(buf) ?: return
        out.add(msg)
    }

    private fun parseMessage(buf: ByteBuffer): OscMessage? {
        val address = readOscString(buf) ?: return null
        return parseMessageBody(buf, address)
    }

    private fun parseMessageBody(buf: ByteBuffer, address: String): OscMessage? {
        if (buf.remaining() == 0) return OscMessage(address, emptyList())
        val typeTag = readOscString(buf) ?: return OscMessage(address, emptyList())
        if (typeTag.isEmpty() || typeTag[0] != ',') return OscMessage(address, emptyList())
        val args = ArrayList<Any>(typeTag.length - 1)
        for (i in 1 until typeTag.length) {
            when (typeTag[i]) {
                'i' -> {
                    if (buf.remaining() < 4) return null
                    args.add(buf.int)
                }
                'f' -> {
                    if (buf.remaining() < 4) return null
                    args.add(buf.float)
                }
                'T' -> args.add(true)
                'F' -> args.add(false)
                's' -> {
                    val s = readOscString(buf) ?: return null
                    args.add(s)
                }
                'd' -> {
                    if (buf.remaining() < 8) return null
                    args.add(buf.double.toFloat())
                }
                else -> {
                    // Unknown type — abort to avoid mis-aligning the stream
                    return OscMessage(address, args)
                }
            }
        }
        return OscMessage(address, args)
    }

    private fun readOscString(buf: ByteBuffer): String? {
        val start = buf.position()
        var end = -1
        for (i in start until buf.limit()) {
            if (buf.get(i) == 0.toByte()) {
                end = i
                break
            }
        }
        if (end < 0) return null
        val s = String(buf.array(), buf.arrayOffset() + start, end - start, Charsets.US_ASCII)
        val padded = ((end - start) + 4) and 3.inv() // round up to multiple of 4 including terminator
        val advance = padded.coerceAtLeast(end - start + 1)
        val newPos = (start + advance).coerceAtMost(buf.limit())
        buf.position(newPos)
        return s
    }

    private fun data(buf: ByteBuffer, idx: Int): Byte = buf.get(idx)
}
