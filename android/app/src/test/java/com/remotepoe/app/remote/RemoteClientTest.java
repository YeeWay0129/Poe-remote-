package com.remotepoe.app.remote;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.view.KeyEvent;
import java.util.ArrayList;
import java.util.List;
import kotlin.Unit;
import org.junit.Test;

public class RemoteClientTest {
    @Test
    public void connectedTransportStartsPeerOfferAndSendsLocalIce() {
        RecordingSignalingTransport transport = new RecordingSignalingTransport();
        FakePeerConnectionGateway peer = new FakePeerConnectionGateway();
        RemoteClient client = new RemoteClient(
            transport,
            peer,
            action -> {
                action.invoke();
                return Unit.INSTANCE;
            }
        );

        assertEquals(ConnectionState.Connected, client.connect("127.0.0.1:7443", "secret"));
        peer.emitLocalOffer("v=0\r\nlocal-offer");
        peer.emitLocalIce(new RemoteIceCandidate("candidate:local", "0", 0));

        assertEquals(1, peer.startCalls);
        assertEquals(1, peer.createOfferCalls);
        assertTrue(transport.snapshotSentJson().stream().anyMatch(json -> json.contains("\"type\":\"offer\"")));
        assertTrue(transport.snapshotSentJson().stream().anyMatch(json -> json.contains("\"type\":\"ice\"")));
    }

    @Test
    public void remoteAnswerAndIceAreAppliedToPeerConnection() {
        RecordingSignalingTransport transport = new RecordingSignalingTransport();
        FakePeerConnectionGateway peer = new FakePeerConnectionGateway();
        RemoteClient client = new RemoteClient(
            transport,
            peer,
            action -> {
                action.invoke();
                return Unit.INSTANCE;
            }
        );

        client.connect("127.0.0.1:7443", "secret");
        transport.receive(
            "{"
                + "\"type\":\"answer\","
                + "\"requestId\":\"answer-1\","
                + "\"payload\":{\"sdp\":\"v=0\\r\\nremote-answer\"}"
                + "}"
        );
        transport.receive(
            "{"
                + "\"type\":\"ice\","
                + "\"requestId\":\"ice-1\","
                + "\"payload\":{"
                + "\"candidate\":\"candidate:remote\","
                + "\"sdpMid\":\"0\","
                + "\"sdpMLineIndex\":0"
                + "}"
                + "}"
        );

        assertEquals("v=0\r\nremote-answer", peer.remoteAnswer);
        assertEquals("candidate:remote", peer.remoteIce.getCandidate());
        assertEquals("0", peer.remoteIce.getSdpMid());
        assertEquals(Integer.valueOf(0), peer.remoteIce.getSdpMLineIndex());
    }

    @Test
    public void inputUsesDataChannelWhenControlChannelAcceptsMessage() {
        RecordingSignalingTransport transport = new RecordingSignalingTransport();
        FakePeerConnectionGateway peer = new FakePeerConnectionGateway();
        RemoteClient client = new RemoteClient(
            transport,
            peer,
            action -> {
                action.invoke();
                return Unit.INSTANCE;
            }
        );
        client.connect("127.0.0.1:7443", "secret");
        int signalingMessagesBeforeInput = transport.snapshotSentJson().size();
        peer.controlMessagesAccepted = true;

        client.sendInput(new RemoteInputEvent.Keyboard(KeyEvent.KEYCODE_W, KeyEvent.ACTION_DOWN));

        assertEquals(signalingMessagesBeforeInput, transport.snapshotSentJson().size());
        assertEquals(1, peer.controlMessages.size());
        assertTrue(peer.controlMessages.get(0).contains("\"type\":\"input_event\""));
        assertTrue(peer.controlMessages.get(0).contains("\"keyCode\":51"));
    }

    @Test
    public void inputFallsBackToSignalingWhenControlChannelRejectsMessage() {
        RecordingSignalingTransport transport = new RecordingSignalingTransport();
        FakePeerConnectionGateway peer = new FakePeerConnectionGateway();
        RemoteClient client = new RemoteClient(
            transport,
            peer,
            action -> {
                action.invoke();
                return Unit.INSTANCE;
            }
        );
        client.connect("127.0.0.1:7443", "secret");
        int signalingMessagesBeforeInput = transport.snapshotSentJson().size();
        peer.controlMessagesAccepted = false;

        client.sendInput(new RemoteInputEvent.Keyboard(KeyEvent.KEYCODE_W, KeyEvent.ACTION_DOWN));

        assertEquals(signalingMessagesBeforeInput + 1, transport.snapshotSentJson().size());
        assertFalse(peer.controlMessages.isEmpty());
        assertTrue(
            transport.snapshotSentJson().get(signalingMessagesBeforeInput).contains("\"type\":\"input_event\"")
        );
    }

    @SuppressWarnings("unchecked")
    private static final class FakePeerConnectionGateway implements PeerConnectionGateway {
        int startCalls = 0;
        int createOfferCalls = 0;
        String remoteAnswer = null;
        RemoteIceCandidate remoteIce = null;
        boolean controlMessagesAccepted = false;
        List<String> controlMessages = new ArrayList<>();

        private kotlin.jvm.functions.Function1<? super String, Unit> onLocalOffer = null;
        private kotlin.jvm.functions.Function1<? super RemoteIceCandidate, Unit> onLocalIceCandidate = null;
        private kotlin.jvm.functions.Function1<? super String, Unit> onError = null;

        @Override
        public kotlin.jvm.functions.Function1<String, Unit> getOnLocalOffer() {
            return (kotlin.jvm.functions.Function1<String, Unit>) onLocalOffer;
        }

        @Override
        public void setOnLocalOffer(kotlin.jvm.functions.Function1<? super String, Unit> callback) {
            onLocalOffer = callback;
        }

        @Override
        public kotlin.jvm.functions.Function1<RemoteIceCandidate, Unit> getOnLocalIceCandidate() {
            return (kotlin.jvm.functions.Function1<RemoteIceCandidate, Unit>) onLocalIceCandidate;
        }

        @Override
        public void setOnLocalIceCandidate(
            kotlin.jvm.functions.Function1<? super RemoteIceCandidate, Unit> callback
        ) {
            onLocalIceCandidate = callback;
        }

        @Override
        public kotlin.jvm.functions.Function1<String, Unit> getOnError() {
            return (kotlin.jvm.functions.Function1<String, Unit>) onError;
        }

        @Override
        public void setOnError(kotlin.jvm.functions.Function1<? super String, Unit> callback) {
            onError = callback;
        }

        @Override
        public void start() {
            startCalls += 1;
        }

        @Override
        public void createOffer() {
            createOfferCalls += 1;
        }

        @Override
        public void setRemoteAnswer(String sdp) {
            remoteAnswer = sdp;
        }

        @Override
        public void addRemoteIceCandidate(RemoteIceCandidate candidate) {
            remoteIce = candidate;
        }

        @Override
        public boolean sendControlMessage(String json) {
            controlMessages.add(json);
            return controlMessagesAccepted;
        }

        @Override
        public void close() {
        }

        void emitLocalOffer(String sdp) {
            if (onLocalOffer != null) onLocalOffer.invoke(sdp);
        }

        void emitLocalIce(RemoteIceCandidate candidate) {
            if (onLocalIceCandidate != null) onLocalIceCandidate.invoke(candidate);
        }
    }
}
