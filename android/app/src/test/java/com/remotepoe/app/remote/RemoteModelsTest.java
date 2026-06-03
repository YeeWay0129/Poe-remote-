package com.remotepoe.app.remote;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;

import android.view.KeyEvent;
import org.junit.Test;

public class RemoteModelsTest {
    @Test
    public void keyboardDownIsAcceptedAsSingleUserAction() {
        assertTrue(RemoteModelsKt.isSingleUserAction(
            new RemoteInputEvent.Keyboard(KeyEvent.KEYCODE_W, KeyEvent.ACTION_DOWN)
        ));
    }

    @Test
    public void zeroMouseMoveIsIgnored() {
        assertFalse(RemoteModelsKt.isSingleUserAction(
            new RemoteInputEvent.MouseMove(0, 0, PointerMode.Relative)
        ));
    }

    @Test
    public void wheelDeltaIsAccepted() {
        assertTrue(RemoteModelsKt.isSingleUserAction(new RemoteInputEvent.MouseWheel(0, -1)));
    }

    @Test
    public void signalingSessionSerializesOfferAndIce() {
        SignalingSession session = new SignalingSession(
            new DeviceIdentity("device-1", "Tablet", "key"),
            StreamConfig.Companion.default1080p60()
        );

        SignalingMessage offer = session.sendOffer("v=0\r\n");
        SignalingMessage ice = session.sendIce("candidate:1", "0", 0);

        assertNotNull(offer);
        assertNotNull(ice);
        assertEquals(SignalingType.Offer, offer.getType());
        assertEquals(SignalingType.Ice, ice.getType());
        assertTrue(SignalingModelsKt.toJsonString(offer).contains("\"sdp\":\"v=0"));
        assertTrue(SignalingModelsKt.toJsonString(ice).contains("\"candidate\":\"candidate:1\""));
    }

    @Test
    public void parserReadsAnswerAndIceMessages() {
        SignalingMessage answer = SignalingModelsKt.parseSignalingMessage(
            "{"
                + "\"type\":\"answer\","
                + "\"requestId\":\"req-answer\","
                + "\"payload\":{\"sdp\":\"v=0\\r\\n\"}"
                + "}"
        );
        SignalingMessage ice = SignalingModelsKt.parseSignalingMessage(
            "{"
                + "\"type\":\"ice\","
                + "\"requestId\":\"req-ice\","
                + "\"payload\":{"
                + "\"candidate\":\"candidate:1\","
                + "\"sdpMid\":\"0\","
                + "\"sdpMLineIndex\":0"
                + "}"
                + "}"
        );

        assertNotNull(answer);
        assertEquals(SignalingType.Answer, answer.getType());
        assertEquals(
            "v=0\r\n",
            ((SignalingPayload.SessionDescription) answer.getPayload()).getSdp()
        );
        assertNotNull(ice);
        assertEquals(
            "candidate:1",
            ((SignalingPayload.Ice) ice.getPayload()).getCandidate()
        );
    }
}
