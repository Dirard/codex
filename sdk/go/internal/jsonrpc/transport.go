package jsonrpc

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sync"
)

// Transport exchanges complete JSON-RPC object frames with no trailing newline
// and no content-length header.
type Transport interface {
	Receive(ctx context.Context) (json.RawMessage, error)
	Send(ctx context.Context, frame json.RawMessage) error
	Close() error
}

type FrameSizeError struct {
	Limit int64
	Size  int64
}

func (e *FrameSizeError) Error() string {
	return fmt.Sprintf("codex frame size error: frame size %d exceeds limit %d", e.Size, e.Limit)
}

type ClosedError struct {
	Reason string
}

func (e *ClosedError) Error() string {
	if e == nil || e.Reason == "" {
		return "codex transport closed"
	}
	return "codex transport closed: " + e.Reason
}

type StreamTransport struct {
	reader *bufio.Reader
	writer io.Writer
	closer io.Closer
	limit  int64

	writeMu sync.Mutex
	closeMu sync.Mutex
	closed  bool
}

func NewStreamTransport(readCloser io.ReadCloser, writeCloser io.WriteCloser, maxFrameBytes int64) *StreamTransport {
	return &StreamTransport{
		reader: bufio.NewReader(readCloser),
		writer: writeCloser,
		closer: closePair{readCloser: readCloser, writeCloser: writeCloser},
		limit:  maxFrameBytes,
	}
}

func (t *StreamTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	stopCancel := context.AfterFunc(ctx, func() { _ = t.Close() })
	defer stopCancel()
	var buf []byte
	for {
		part, err := t.reader.ReadSlice('\n')
		buf = append(buf, part...)
		payloadLen := int64(len(bytes.TrimRight(buf, "\r\n")))
		if t.limit > 0 && payloadLen > t.limit {
			_ = t.Close()
			return nil, &FrameSizeError{Limit: t.limit, Size: payloadLen}
		}
		if errors.Is(err, bufio.ErrBufferFull) {
			continue
		}
		if err != nil {
			if errors.Is(err, io.EOF) && len(buf) > 0 {
				break
			}
			if t.isClosed() {
				if ctxErr := ctx.Err(); ctxErr != nil {
					return nil, ctxErr
				}
				return nil, &ClosedError{}
			}
			return nil, err
		}
		break
	}
	frame := bytes.TrimRight(buf, "\r\n")
	if len(bytes.TrimSpace(frame)) == 0 {
		return nil, fmt.Errorf("empty JSON-RPC frame")
	}
	return json.RawMessage(append([]byte(nil), frame...)), nil
}

func (t *StreamTransport) Send(ctx context.Context, frame json.RawMessage) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if t.limit > 0 && int64(len(frame)) > t.limit {
		return &FrameSizeError{Limit: t.limit, Size: int64(len(frame))}
	}
	if t.isClosed() {
		return &ClosedError{}
	}
	stopCancel := context.AfterFunc(ctx, func() { _ = t.Close() })
	defer stopCancel()
	t.writeMu.Lock()
	defer t.writeMu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	if t.isClosed() {
		return &ClosedError{}
	}
	if _, err := t.writer.Write(frame); err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return ctxErr
		}
		return err
	}
	if _, err := t.writer.Write([]byte{'\n'}); err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return ctxErr
		}
		return err
	}
	return nil
}

func (t *StreamTransport) Close() error {
	t.closeMu.Lock()
	defer t.closeMu.Unlock()
	if t.closed {
		return nil
	}
	t.closed = true
	return t.closer.Close()
}

func (t *StreamTransport) isClosed() bool {
	t.closeMu.Lock()
	defer t.closeMu.Unlock()
	return t.closed
}

type closePair struct {
	readCloser  io.Closer
	writeCloser io.Closer
}

func (p closePair) Close() error {
	err1 := p.readCloser.Close()
	err2 := p.writeCloser.Close()
	if err1 != nil {
		return err1
	}
	return err2
}
