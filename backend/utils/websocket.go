package utils

import (
	"bufio"
	"crypto/sha1"
	"encoding/base64"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
)

const wsHandshakeGUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

// OpCode represents a WebSocket frame opcode.
type OpCode byte

const (
	OpContinuation OpCode = 0x0
	OpText         OpCode = 0x1
	OpBinary       OpCode = 0x2
	OpClose        OpCode = 0x8
	OpPing         OpCode = 0x9
	OpPong         OpCode = 0xA
)

// WSConn is a minimal WebSocket connection backed by a hijacked net.Conn.
// It implements the RFC 6455 framing protocol using only the standard library.
type WSConn struct {
	conn   net.Conn
	reader *bufio.Reader
	closed bool
}

// UpgradeHTTP performs the WebSocket handshake over an existing HTTP connection
// by hijacking the underlying TCP socket. No external dependencies required.
func UpgradeHTTP(w http.ResponseWriter, r *http.Request) (*WSConn, error) {
	if !strings.EqualFold(r.Header.Get("Upgrade"), "websocket") {
		return nil, fmt.Errorf("not a websocket upgrade request")
	}
	key := r.Header.Get("Sec-WebSocket-Key")
	if key == "" {
		return nil, fmt.Errorf("missing Sec-WebSocket-Key header")
	}

	hj, ok := w.(http.Hijacker)
	if !ok {
		return nil, fmt.Errorf("response writer does not support hijacking")
	}

	conn, bufrw, err := hj.Hijack()
	if err != nil {
		return nil, fmt.Errorf("hijack failed: %w", err)
	}

	accept := computeAcceptKey(key)
	handshake := "HTTP/1.1 101 Switching Protocols\r\n" +
		"Upgrade: websocket\r\n" +
		"Connection: Upgrade\r\n" +
		"Sec-WebSocket-Accept: " + accept + "\r\n\r\n"

	if _, err := bufrw.WriteString(handshake); err != nil {
		conn.Close()
		return nil, fmt.Errorf("failed to write handshake: %w", err)
	}
	if err := bufrw.Flush(); err != nil {
		conn.Close()
		return nil, fmt.Errorf("failed to flush handshake: %w", err)
	}

	return &WSConn{conn: conn, reader: bufrw.Reader}, nil
}

// WriteMessage sends a text frame to the remote peer.
func (c *WSConn) WriteMessage(msg []byte) error {
	if c.closed {
		return fmt.Errorf("connection is closed")
	}
	return writeFrame(c.conn, OpText, msg)
}

// WritePong sends a pong control frame.
func (c *WSConn) WritePong(payload []byte) error {
	return writeFrame(c.conn, OpPong, payload)
}

// ReadMessage reads the next WebSocket frame from the remote peer.
func (c *WSConn) ReadMessage() (OpCode, []byte, error) {
	return readFrame(c.reader)
}

// Close sends a close frame and shuts down the underlying connection.
func (c *WSConn) Close() {
	if !c.closed {
		c.closed = true
		_ = writeFrame(c.conn, OpClose, []byte{})
		c.conn.Close()
	}
}

// IsClosed reports whether the connection has been closed.
func (c *WSConn) IsClosed() bool { return c.closed }

// ---- internal helpers -------------------------------------------------------

func computeAcceptKey(key string) string {
	h := sha1.New()
	h.Write([]byte(key + wsHandshakeGUID))
	return base64.StdEncoding.EncodeToString(h.Sum(nil))
}

func writeFrame(w io.Writer, op OpCode, payload []byte) error {
	// First byte: FIN=1 + opcode
	header := []byte{0x80 | byte(op)}

	// Second byte (+ extended length): no masking for server->client frames
	l := len(payload)
	switch {
	case l < 126:
		header = append(header, byte(l))
	case l < 65536:
		ext := make([]byte, 2)
		binary.BigEndian.PutUint16(ext, uint16(l))
		header = append(header, 126)
		header = append(header, ext...)
	default:
		ext := make([]byte, 8)
		binary.BigEndian.PutUint64(ext, uint64(l))
		header = append(header, 127)
		header = append(header, ext...)
	}

	if _, err := w.Write(header); err != nil {
		return err
	}
	if len(payload) > 0 {
		_, err := w.Write(payload)
		return err
	}
	return nil
}

func readFrame(r io.Reader) (OpCode, []byte, error) {
	header := make([]byte, 2)
	if _, err := io.ReadFull(r, header); err != nil {
		return 0, nil, err
	}

	op := OpCode(header[0] & 0x0F)
	masked := (header[1] >> 7) & 1
	payloadLen := int64(header[1] & 0x7F)

	if payloadLen == 126 {
		ext := make([]byte, 2)
		if _, err := io.ReadFull(r, ext); err != nil {
			return 0, nil, err
		}
		payloadLen = int64(binary.BigEndian.Uint16(ext))
	} else if payloadLen == 127 {
		ext := make([]byte, 8)
		if _, err := io.ReadFull(r, ext); err != nil {
			return 0, nil, err
		}
		payloadLen = int64(binary.BigEndian.Uint64(ext))
	}

	var maskKey []byte
	if masked == 1 {
		maskKey = make([]byte, 4)
		if _, err := io.ReadFull(r, maskKey); err != nil {
			return 0, nil, err
		}
	}

	payload := make([]byte, payloadLen)
	if payloadLen > 0 {
		if _, err := io.ReadFull(r, payload); err != nil {
			return 0, nil, err
		}
		if masked == 1 {
			for i := range payload {
				payload[i] ^= maskKey[i%4]
			}
		}
	}

	return op, payload, nil
}
